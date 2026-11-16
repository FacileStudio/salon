//! When a room saves: a flush after accepted updates, a snapshot past a threshold, when the
//! room empties and when it stops.

use tokio::time::Instant;

use crate::hooks::{Due, Host, Saved};
use crate::room::{Owed, Room};

impl<H: Host> Room<H> {
    /// Whether a snapshot is owed now.
    pub(crate) fn snapshot_due(&self, now: Instant) -> bool {
        let (owed, policy) = (&self.owed, &self.policy);
        let late = owed.since.zip(policy.snapshot_after).is_some_and(|(at, after)| at + after <= now);
        owed.count >= policy.snapshot_every || owed.bytes >= policy.snapshot_bytes || late
    }

    async fn save(&mut self, due: Due) {
        let author = self.owed.author;
        match self.host.save(self.key, &mut self.space, due, author).await {
            Saved::Flushed => (self.owed.unflushed, self.owed.flush_at) = (false, None),
            Saved::Stored => self.owed = Owed::default(),
            Saved::Retry => self.owed.flush_at = Some(Instant::now() + self.policy.retry),
            Saved::Gone => self.vanish(),
        }
    }

    /// Stores the updates accepted since the last save.
    pub(crate) async fn flush(&mut self) {
        self.owed.flush_at = None;
        if self.owed.unflushed {
            self.save(Due::Flush).await;
        }
    }

    /// Stores the whole state, when anything is owed.
    pub(crate) async fn snapshot(&mut self) {
        if self.owes() {
            self.save(Due::Snapshot).await;
        }
    }

    /// Saves as `due` asks, for a server-side write.
    pub(crate) async fn settle(&mut self, due: Due) {
        match due {
            Due::Flush => self.flush().await,
            Due::Snapshot => self.snapshot().await,
        }
    }
}
