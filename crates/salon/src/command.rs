//! What the hub tells a room task, and what a room writes to a connection.

use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::error::Fault;
use crate::hooks::Due;

/// A frame for one connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Out {
    /// A y-websocket message.
    Binary(Vec<u8>),
    /// Close the socket with this code and reason.
    Close(u16, String),
}

/// Who a connection speaks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Who {
    /// The signed-in user; `None` for a guest.
    pub user_id: Option<i64>,
    /// The share link a guest came through.
    pub share: Option<Uuid>,
    /// The name the server writes into the member's presence `user.name`; `None` leaves the
    /// member's own.
    pub name: Option<String>,
    /// Whether its document updates are applied; a viewer sends presence only.
    pub writer: bool,
}

/// A connection joining a room.
#[derive(Debug, Clone)]
pub struct Member {
    /// The connection id, from [`crate::Hub::next_id`].
    pub conn: u64,
    /// Who it speaks for.
    pub who: Who,
    /// Where the room writes to it.
    pub outbox: mpsc::Sender<Out>,
}

/// A command answer. A dropped sender means the room unloaded first: the hub retries.
pub(crate) type Reply<T> = oneshot::Sender<Result<T, Fault>>;

/// How an edit ended, for the closure that answers its caller.
pub(crate) enum Outcome {
    /// The edit was refused and changed nothing.
    Refused,
    /// The edit held; `true` when it is stored.
    Done(bool),
    /// The document went away while it was stored.
    Ended,
    /// The room could not load the document.
    Failed(Fault),
}

/// The edit half of a write: runs on the document, answers the update to relay.
pub(crate) type EditFn<S> = Box<dyn FnOnce(&mut S) -> Result<Option<Vec<u8>>, ()> + Send>;
/// The answer half of a write: reads the document after the save (absent when the room could
/// not load it) and answers the caller.
pub(crate) type AnswerFn<S> = Box<dyn FnOnce(Option<&S>, Outcome) + Send>;
/// A read: runs on the live document, or answers the load failure.
pub(crate) type ReadFn<S> = Box<dyn FnOnce(Result<&S, Fault>) + Send>;

/// A server-side write. A room that unloads first sends it back through `bounce`.
pub(crate) struct Job<S> {
    pub(crate) edit: EditFn<S>,
    pub(crate) answer: AnswerFn<S>,
    pub(crate) due: Due,
    pub(crate) author: Option<i64>,
    pub(crate) bounce: Option<oneshot::Sender<Job<S>>>,
}

/// One instruction to a room task.
pub(crate) enum Command<S> {
    /// A connection joins and receives the opening sync messages.
    Join(Member, Reply<()>),
    /// A binary frame from a connection.
    Message(u64, Vec<u8>),
    /// A connection left.
    Leave(u64),
    /// A read of the live document.
    Read(ReadFn<S>),
    /// A server-side write, validated, relayed and saved before its answer.
    Write(Job<S>),
    /// A message for every member.
    Broadcast(Vec<u8>),
    /// Close the connections of these users that lost access.
    Evict(Vec<i64>),
    /// Close every connection that came through this share link.
    Revoke(Uuid),
    /// Save, close every member with this code and reason, and unload; answers the connections
    /// it closed.
    Shutdown(u16, String, oneshot::Sender<Vec<u64>>),
}
