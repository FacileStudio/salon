//! y-websocket messages: sync steps, updates, presence and custom notices.

use yrs::sync::{Awareness, AwarenessUpdate, Message, SyncMessage};
use yrs::updates::decoder::DecoderV1;
use yrs::updates::encoder::Encode;
use yrs::{ReadTxn, Transact};

use crate::error::Refusal;
use crate::space::Space;

/// The y-websocket message type of the server's custom notices.
pub const CUSTOM: u8 = 100;

/// What one message asks the room to do.
#[derive(Debug)]
pub(crate) enum Effect {
    /// Send back to the sender.
    Reply(Vec<u8>),
    /// An accepted update, encoded, and the length of the update itself: relay it, save it soon.
    Accepted(Vec<u8>, usize),
    /// A presence change for the room to vet and relay.
    Presence(AwarenessUpdate),
    /// Close the sender's socket with this code.
    Refuse(u16, String),
}

/// Encodes one y-websocket message.
pub fn encode(message: Message) -> Vec<u8> {
    message.encode_v1()
}

/// A custom notice: message type [`CUSTOM`] carrying `json`, such as
/// `{"type":"comments-changed"}`.
pub fn custom(json: &str) -> Vec<u8> {
    encode(Message::Custom(CUSTOM, json.as_bytes().to_vec()))
}

/// What a joining member receives: the server's state vector, then everyone's presence.
pub(crate) fn opening(space: &impl Space, awareness: &Awareness) -> Vec<Vec<u8>> {
    let sv = space.doc().transact().state_vector();
    let mut out = vec![encode(Message::Sync(SyncMessage::SyncStep1(sv)))];
    if let Some(update) = awareness.update().ok().filter(|u| !u.clients.is_empty()) {
        out.push(encode(Message::Awareness(update)));
    }
    out
}

fn on_update(space: &mut impl Space, update: Vec<u8>, writer: bool) -> Option<Effect> {
    if !writer {
        return None;
    }
    match space.apply(&update) {
        Ok(true) => {
            let len = update.len();
            Some(Effect::Accepted(encode(Message::Sync(SyncMessage::Update(update))), len))
        }
        Ok(false) => None,
        Err(Refusal(code, reason)) => Some(Effect::Refuse(code, reason)),
    }
}

fn on_message(space: &mut impl Space, awareness: &Awareness, message: Message, writer: bool) -> Option<Effect> {
    match message {
        Message::Sync(SyncMessage::SyncStep1(sv)) => {
            let update = space.doc().transact().encode_state_as_update_v1(&sv);
            Some(Effect::Reply(encode(Message::Sync(SyncMessage::SyncStep2(update)))))
        }
        Message::Sync(SyncMessage::SyncStep2(update) | SyncMessage::Update(update)) => on_update(space, update, writer),
        Message::Awareness(update) => Some(Effect::Presence(update)),
        Message::AwarenessQuery => {
            awareness.update().ok().map(|update| Effect::Reply(encode(Message::Awareness(update))))
        }
        Message::Auth(_) | Message::Custom(..) => None,
    }
}

/// Handles one binary frame from a member. A viewer's document updates are dropped; its
/// presence is relayed. Processing stops at the first refusal.
pub(crate) fn handle(space: &mut impl Space, awareness: &Awareness, frame: &[u8], writer: bool) -> Vec<Effect> {
    let mut decoder = DecoderV1::from(frame);
    let mut effects = Vec::new();
    for message in yrs::sync::protocol::MessageReader::new(&mut decoder) {
        let Ok(message) = message else {
            effects.push(Effect::Refuse(1008, "malformed y-websocket frame".to_owned()));
            break;
        };
        effects.extend(on_message(space, awareness, message, writer));
        if matches!(effects.last(), Some(Effect::Refuse(..))) {
            break;
        }
    }
    effects
}

/// [`handle`], turning a panic into a 1011 refusal after [`Space::recover`].
pub(crate) fn guarded(space: &mut impl Space, awareness: &Awareness, frame: &[u8], writer: bool) -> Vec<Effect> {
    let run = std::panic::AssertUnwindSafe(|| handle(&mut *space, awareness, frame, writer));
    std::panic::catch_unwind(run).unwrap_or_else(|_| {
        let reason = match space.recover() {
            Ok(()) => "update failed",
            Err(_) => "room failed",
        };
        vec![Effect::Refuse(1011, reason.to_owned())]
    })
}
