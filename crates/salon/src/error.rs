//! What can go wrong: a refused update, which closes its sender, and a fault, which answers a
//! server-side call.

/// Why an update was refused: the close code and reason for its sender. The document is back
/// to exactly the state before the update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(pub u16, pub String);

/// The kind of a [`Fault`], for the app to map onto its own error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The document is gone.
    NotFound,
    /// The request was refused as invalid.
    Invalid,
    /// The document is in a state that refuses the request.
    Failed,
    /// The room is restarting; the caller may retry.
    Unavailable,
    /// The room or its store failed.
    Internal,
}

/// A failure answered to a server-side call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    /// What kind of failure.
    pub kind: Kind,
    /// A message for people.
    pub message: String,
}

impl Fault {
    /// A fault of `kind` saying `message`.
    pub fn new(kind: Kind, message: impl Into<String>) -> Fault {
        Fault { kind, message: message.into() }
    }
}
