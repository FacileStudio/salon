//! How the hub talks to its rooms: commands that answer, commands that do not, and the notices
//! an app sends its open rooms.

use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::command::{Command, Reply};
use crate::error::{Fault, Kind};
use crate::hooks::Host;
use crate::hub::Hub;

/// The fault a caller gets when a room kept unloading under it.
pub(crate) fn restarting(noun: &str) -> Fault {
    Fault::new(Kind::Unavailable, format!("the {noun} room is restarting; retry"))
}

impl<H: Host> Hub<H> {
    /// Sends a command that answers, loading the room if needed and retrying on a room that
    /// was unloading.
    pub(crate) async fn ask<T>(
        &self,
        host: &H,
        key: Uuid,
        mut make: impl FnMut(Reply<T>) -> Command<H::Space>,
    ) -> Result<T, Fault> {
        for _ in 0..3 {
            let (reply, answer) = oneshot::channel();
            if self.sender(host, key).send(make(reply)).await.is_err() {
                continue;
            }
            if let Ok(result) = answer.await {
                return result;
            }
        }
        Err(restarting(host.rooms().1.noun))
    }

    /// Sends a command to room `key` when it is open; a closed room drops it.
    pub(crate) async fn tell(&self, key: Uuid, command: Command<H::Space>) {
        let sender = self.lock().get(&key).map(|(_, sender)| sender.clone());
        if let Some(sender) = sender {
            _ = sender.send(command).await;
        }
    }

    /// The senders of every open room, or of the rooms of `keys`.
    pub(crate) fn senders(&self, keys: Option<&[Uuid]>) -> Vec<mpsc::Sender<Command<H::Space>>> {
        let rooms = self.lock();
        match keys {
            Some(keys) => keys.iter().filter_map(|key| rooms.get(key).map(|(_, s)| s.clone())).collect(),
            None => rooms.values().map(|(_, sender)| sender.clone()).collect(),
        }
    }

    /// Sends `frame` to every member of room `key`, when it is open.
    pub async fn broadcast(&self, key: Uuid, frame: Vec<u8>) {
        self.tell(key, Command::Broadcast(frame)).await;
    }

    /// Closes, in every open room, the connections of `users` that lost access.
    pub async fn evict(&self, users: &[i64]) {
        for sender in self.senders(None) {
            _ = sender.send(Command::Evict(users.to_vec())).await;
        }
    }

    /// Closes every connection of room `key` that came through share link `share`.
    pub async fn revoke(&self, key: Uuid, share: Uuid) {
        self.tell(key, Command::Revoke(share)).await;
    }
}
