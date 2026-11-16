//! Document work off the async threads, for documents whose check is heavy.

use yrs::Doc;
use yrs::sync::Awareness;

use crate::hooks::Host;
use crate::room::Room;

impl<H: Host> Room<H> {
    /// Runs `work` on the document and presence, on a blocking thread when the policy asks.
    /// The document is always given back; `None` means the blocking task did not finish and the
    /// document is lost, so the caller must unload without saving.
    pub(crate) async fn lend<T: Send + 'static>(
        &mut self,
        work: impl FnOnce(&mut H::Space, &Awareness) -> T + Send + 'static,
    ) -> Option<T> {
        if !self.policy.blocking {
            return Some(work(&mut self.space, &self.awareness));
        }
        let mut space = std::mem::take(&mut self.space);
        let awareness = std::mem::replace(&mut self.awareness, Awareness::new(Doc::new()));
        let joined = tokio::task::spawn_blocking(move || {
            let out = work(&mut space, &awareness);
            (space, awareness, out)
        })
        .await;
        let (space, awareness, out) = joined.ok()?;
        (self.space, self.awareness) = (space, awareness);
        Some(out)
    }
}
