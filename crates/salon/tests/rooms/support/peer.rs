//! A y-websocket client for the tests.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as Frame;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use yrs::sync::awareness::{AwarenessUpdate, AwarenessUpdateEntry};
use yrs::sync::{Message, SyncMessage};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{ClientID, Doc, Map, ReadTxn, Transact, Update};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// How long a pump waits for the next frame before it calls the line quiet.
pub const QUIET: Duration = Duration::from_millis(250);

/// One member socket and its copy of the document.
pub struct Peer {
    pub doc: Doc,
    socket: Option<Socket>,
    pub closed: Option<(u16, String)>,
    pub presence: Vec<String>,
}

impl Peer {
    /// Connects to `url` and runs the opening sync.
    pub async fn connect(url: &str) -> Peer {
        let (socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let mut peer = Peer { doc: Doc::new(), socket: Some(socket), closed: None, presence: vec![] };
        let sv = peer.doc.transact().state_vector();
        peer.send(Message::Sync(SyncMessage::SyncStep1(sv))).await;
        peer.pump().await;
        peer
    }

    /// Sends one message.
    pub async fn send(&mut self, message: Message) {
        if let Some(socket) = self.socket.as_mut() {
            _ = socket.send(Frame::Binary(message.encode_v1().into())).await;
        }
    }

    /// Sets `k` to `v` in map `m` and sends the update.
    pub async fn set(&mut self, k: &str, v: &str) {
        let map = self.doc.get_or_insert_map("m");
        let update = {
            let mut txn = self.doc.transact_mut();
            map.insert(&mut txn, k, v);
            txn.encode_update_v1()
        };
        self.send(Message::Sync(SyncMessage::Update(update))).await;
    }

    /// Announces presence for Yjs client `id` under `name`.
    pub async fn announce(&mut self, id: u64, name: &str) {
        let entry = AwarenessUpdateEntry { clock: 1, json: format!("{{\"user\":{{\"name\":\"{name}\"}}}}").into() };
        let update = AwarenessUpdate { clients: HashMap::from([(ClientID::new(id), entry)]) };
        self.send(Message::Awareness(update)).await;
    }

    fn on(&mut self, message: Message) -> Option<Message> {
        match message {
            Message::Sync(SyncMessage::SyncStep1(sv)) => {
                Some(Message::Sync(SyncMessage::SyncStep2(self.doc.transact().encode_state_as_update_v1(&sv))))
            }
            Message::Sync(SyncMessage::SyncStep2(u) | SyncMessage::Update(u)) => {
                self.doc.transact_mut().apply_update(Update::decode_v1(&u).unwrap()).unwrap();
                None
            }
            Message::Awareness(update) => {
                self.presence.extend(update.clients.values().map(|entry| entry.json.to_string()));
                None
            }
            _ => None,
        }
    }

    /// Reads frames until the line is quiet for [`QUIET`] or the socket closes.
    pub async fn pump(&mut self) {
        while let Some(socket) = self.socket.as_mut() {
            let Ok(frame) = tokio::time::timeout(QUIET, socket.next()).await else { return };
            match frame {
                Some(Ok(Frame::Binary(bytes))) => {
                    let mut decoder = yrs::updates::decoder::DecoderV1::from(bytes.as_ref());
                    let messages: Vec<Message> =
                        yrs::sync::protocol::MessageReader::new(&mut decoder).filter_map(Result::ok).collect();
                    for reply in messages.into_iter().filter_map(|m| self.on(m)).collect::<Vec<_>>() {
                        self.send(reply).await;
                    }
                }
                Some(Ok(Frame::Close(frame))) => {
                    let close = frame.map_or((1005, String::new()), |f| (u16::from(f.code), f.reason.to_string()));
                    self.closed.get_or_insert(close);
                    self.socket = None;
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => {
                    self.closed.get_or_insert((1006, String::new()));
                    self.socket = None;
                }
            }
        }
    }
}
