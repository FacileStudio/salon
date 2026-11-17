//! Live co-editing rooms for Facile Suite apps.
//!
//! One room per open document: an in-memory Yjs document that every connected member edits
//! over y-websocket. salon owns the generic parts (the hub of rooms, the room loop, sync,
//! presence, slow members, socket pumping, session re-checks, shutdown). The app owns its data
//! and its rules through two hooks: [`Host`], implemented on the app state, and [`Space`], the
//! document of one room. The wire behaviour is specified in `spec/PROTOCOL.md`.
#![deny(unused, dead_code, missing_docs)]

mod calls;
mod command;
mod drain;
mod error;
mod hooks;
mod hub;
mod lend;
mod members;
mod persist;
mod presence;
mod room;
mod sockets;
mod space;
mod sync;
mod writes;
mod ws;

pub use command::{Member, Out, Who};
pub use drain::SHUTDOWN_WAIT;
pub use error::{Fault, Kind, Refusal};
pub use hooks::{Due, Host, Policy, Saved};
pub use hub::Hub;
pub use presence::{MAX_CLIENTS, stamp};
pub use space::Space;
pub use sync::{CUSTOM, custom, encode};
pub use writes::Change;
pub use ws::{Caller, MAX_FRAME, MAX_REASON, OUTBOX, accept, close};
