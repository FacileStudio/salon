//! The hub: every open room of one app, keyed by document id. One replica only, since two
//! would split a room.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::mpsc;
use uuid::Uuid;

use crate::command::Command;
use crate::hooks::Host;
use crate::room;

/// How many commands wait for one room before senders block.
pub const QUEUE: usize = 1024;

type Rooms<S> = HashMap<Uuid, (u64, mpsc::Sender<Command<S>>)>;

/// Every open room of an app. Cloning shares the same rooms.
pub struct Hub<H: Host> {
    rooms: Arc<Mutex<Rooms<H::Space>>>,
    ids: Arc<AtomicU64>,
}

impl<H: Host> Clone for Hub<H> {
    fn clone(&self) -> Self {
        Hub { rooms: self.rooms.clone(), ids: self.ids.clone() }
    }
}

impl<H: Host> Default for Hub<H> {
    fn default() -> Self {
        Hub { rooms: Arc::default(), ids: Arc::default() }
    }
}

impl<H: Host> Hub<H> {
    /// A fresh id, for a connection or a room generation.
    pub fn next_id(&self) -> u64 {
        self.ids.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Rooms<H::Space>> {
        self.rooms.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The sender of room `key`, loading the room when it is not open.
    pub(crate) fn sender(&self, host: &H, key: Uuid) -> mpsc::Sender<Command<H::Space>> {
        let mut rooms = self.lock();
        if let Some((_, sender)) = rooms.get(&key).filter(|(_, sender)| !sender.is_closed()) {
            return sender.clone();
        }
        let (sender, receiver) = mpsc::channel(QUEUE);
        let generation = self.next_id();
        rooms.insert(key, (generation, sender.clone()));
        tokio::spawn(room::run(host.clone(), key, generation, receiver));
        sender
    }

    /// Drops room `key` of `generation` from the map; a newer room is left alone.
    pub(crate) fn forget(&self, key: Uuid, generation: u64) {
        let mut rooms = self.lock();
        if rooms.get(&key).is_some_and(|(current, _)| *current == generation) {
            rooms.remove(&key);
        }
    }
}
