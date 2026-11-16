//! Presence (awareness): relayed, never stored. A member speaks only for its own Yjs clients,
//! and when the app names it, its `user.name` and `user.guest` are the server's, not its own.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;
use yrs::ClientID;
use yrs::sync::{Awareness, AwarenessUpdate, Message};

use crate::command::{Out, Who};
use crate::hooks::Host;
use crate::room::Room;
use crate::sync::encode;

/// The most Yjs clients one connection may announce.
pub const MAX_CLIENTS: usize = 8;

/// `json` with `user.name` and `user.guest` set from `who` when `who` carries a name; anything
/// that is not an object with a `user` object passes unchanged (`null` is a client leaving).
pub fn stamp(json: &str, who: &Who) -> Arc<str> {
    let Some(name) = &who.name else { return json.into() };
    let Ok(Value::Object(mut state)) = serde_json::from_str::<Value>(json) else { return json.into() };
    let Some(Value::Object(user)) = state.get_mut("user") else { return json.into() };
    user.insert("name".to_owned(), Value::String(name.clone()));
    user.insert("guest".to_owned(), Value::Bool(who.user_id.is_none()));
    Value::Object(state).to_string().into()
}

/// The presence update that tells everyone `clients` left.
pub(crate) fn departure(awareness: &mut Awareness, clients: &HashSet<ClientID>) -> Option<Vec<u8>> {
    clients.iter().for_each(|client| awareness.remove_state(*client));
    let update = awareness.update_with_clients(clients.iter().copied()).ok()?;
    (!update.clients.is_empty()).then(|| encode(Message::Awareness(update)))
}

impl<H: Host> Room<H> {
    /// Vets a member's presence update, applies it and relays it to the others.
    pub(crate) fn presence(&mut self, conn: u64, mut update: AwarenessUpdate) {
        let taken: HashSet<ClientID> =
            self.conns.iter().filter(|(id, _)| **id != conn).flat_map(|(_, c)| c.clients.iter().copied()).collect();
        update.clients.retain(|client, _| !taken.contains(client));
        let Some(member) = self.conns.get_mut(&conn) else { return };
        member.clients.extend(update.clients.keys().copied());
        if member.clients.len() > MAX_CLIENTS {
            let reason = format!("more than {MAX_CLIENTS} presence clients");
            return self.drop_conn(conn, Some(Out::Close(1008, reason)));
        }
        let who = member.who.clone();
        update.clients.values_mut().for_each(|entry| entry.json = stamp(&entry.json, &who));
        if update.clients.is_empty() || self.awareness.apply_update(update.clone()).is_err() {
            return;
        }
        self.broadcast(Some(conn), &encode(Message::Awareness(update)));
    }
}
