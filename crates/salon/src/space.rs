//! The document of one room, as the app defines it.

use yrs::Doc;

use crate::error::Refusal;

/// The document of one room. It is the only thing a room changes, and it must stay valid: a
/// refused update or edit leaves it exactly as it was.
pub trait Space: Default + Send + 'static {
    /// The live Yjs document.
    fn doc(&self) -> &Doc;

    /// Applies a member's v1 `update` and keeps it only when the document still validates.
    /// Answers whether the update held anything to relay.
    fn apply(&mut self, update: &[u8]) -> Result<bool, Refusal>;

    /// Restores a valid document after a panic in [`Space::apply`] or an edit.
    fn recover(&mut self) -> Result<(), Refusal> {
        Ok(())
    }

    /// Whether members may write; `false` makes everyone a viewer (a document in the trash).
    fn writable(&self) -> bool {
        true
    }
}
