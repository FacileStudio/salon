//! The two hooks an app implements: [`Host`] on its state, [`Space`] for one room's document.

use std::future::Future;
use std::time::Duration;

use tokio::time::Instant;
use uuid::Uuid;

use crate::error::Fault;
use crate::hub::Hub;
use crate::space::Space;
use crate::ws::Caller;

/// What a save covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    /// Store what was accepted since the last save. Cheap and frequent.
    Flush,
    /// Store the whole state: the room is idle, stopping, or past a snapshot threshold.
    Snapshot,
}

/// What a save did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Saved {
    /// The accepted updates are stored; a snapshot is still owed.
    Flushed,
    /// The whole state is stored: nothing is owed.
    Stored,
    /// The store failed; the room tries again after [`Policy::retry`].
    Retry,
    /// The document is gone: the room closes every member and unloads.
    Gone,
}

/// When a room saves, unloads and re-checks its sockets, and the words it closes with.
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    /// How long accepted updates wait before a [`Due::Flush`].
    pub flush: Duration,
    /// Whether every accepted update pushes the flush back (a debounce) instead of the first one
    /// setting it.
    pub debounce: bool,
    /// How long after a failed save the room tries again.
    pub retry: Duration,
    /// A snapshot is owed once this many updates were accepted since the last one.
    pub snapshot_every: usize,
    /// A snapshot is owed once the updates accepted since the last one weigh this much.
    pub snapshot_bytes: usize,
    /// A snapshot is owed once the oldest update since the last one is this old.
    pub snapshot_after: Option<Duration>,
    /// How long an empty room with nothing owed stays loaded.
    pub idle: Duration,
    /// How often a socket re-checks that its caller may stay.
    pub recheck: Duration,
    /// Whether document work runs on a blocking thread, for documents whose check is heavy.
    pub blocking: bool,
    /// The document noun in messages: `board`, `document`.
    pub noun: &'static str,
}

/// The app side of the rooms, implemented on the app state.
pub trait Host: Clone + Send + Sync + 'static {
    /// One room's document.
    type Space: Space;
    /// The error the app answers server-side calls with.
    type Error: Send + 'static;

    /// The hub of this app's rooms, and its policy.
    fn rooms(&self) -> (&Hub<Self>, Policy);

    /// The app error for a salon [`Fault`].
    fn fault(fault: Fault) -> Self::Error;

    /// Loads the document of room `key`.
    fn open(&self, key: Uuid) -> impl Future<Output = Result<Self::Space, Fault>> + Send;

    /// Stores `space` as `due` asks. `author` is the last member who changed it, when known.
    fn save(
        &self,
        key: Uuid,
        space: &mut Self::Space,
        due: Due,
        author: Option<i64>,
    ) -> impl Future<Output = Saved> + Send;

    /// Whether `caller` may keep its socket: `None` keeps it, a code and reason close it.
    fn alive(&self, caller: &Caller, opened: Instant) -> impl Future<Output = Option<(u16, String)>> + Send;

    /// Whether `user` lost all access to room `key`.
    fn lost(&self, key: Uuid, user: i64) -> impl Future<Output = bool> + Send;
}
