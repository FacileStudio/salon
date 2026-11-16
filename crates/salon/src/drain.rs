//! Rooms that stop: commands left behind, documents that fail to load, close and shutdown.

use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::command::{Command, Outcome};
use crate::error::Fault;
use crate::hooks::Host;
use crate::hub::Hub;
use crate::room::Room;

/// Answers a command that reached a room after it stopped. A dropped reply makes the hub retry
/// on a fresh room; a write goes back to its caller to be sent again.
pub(crate) fn answer_gone<S>(command: Command<S>) {
    match command {
        Command::Write(mut job) => {
            if let Some(bounce) = job.bounce.take() {
                _ = bounce.send(job);
            }
        }
        Command::Shutdown(_, _, done) => _ = done.send(()),
        Command::Join(..)
        | Command::Read(_)
        | Command::Message(..)
        | Command::Leave(_)
        | Command::Broadcast(_)
        | Command::Evict(_)
        | Command::Revoke(_) => {}
    }
}

/// Answers every command with `fault`: the document could not be loaded.
pub(crate) async fn refuse_all<H: Host>(
    host: &H,
    key: Uuid,
    generation: u64,
    mut commands: mpsc::Receiver<Command<H::Space>>,
    fault: Fault,
) {
    host.rooms().0.forget(key, generation);
    commands.close();
    while let Some(command) = commands.recv().await {
        match command {
            Command::Join(_, reply) => _ = reply.send(Err(fault.clone())),
            Command::Read(read) => read(Err(fault.clone())),
            Command::Write(job) => (job.answer)(None, Outcome::Failed(fault.clone())),
            other => answer_gone(other),
        }
    }
}

async fn stop_all<S>(senders: Vec<mpsc::Sender<Command<S>>>, code: u16, reason: &str) {
    for sender in senders {
        let (done, wait) = oneshot::channel();
        if sender.send(Command::Shutdown(code, reason.to_owned(), done)).await.is_ok() {
            _ = wait.await;
        }
    }
}

impl<H: Host> Hub<H> {
    /// Saves and unloads the rooms of `keys`, closing their members with `code` and `reason`:
    /// the documents were trashed, restored or deleted.
    pub async fn close(&self, keys: &[Uuid], code: u16, reason: &str) {
        stop_all(self.senders(Some(keys)), code, reason).await;
    }

    /// Saves and unloads every room, closing members with `code` and `reason`, for a graceful
    /// stop.
    pub async fn shutdown(&self, code: u16, reason: &str) {
        stop_all(self.senders(None), code, reason).await;
    }
}

impl<H: Host> Room<H> {
    /// Snapshots, then closes every member with `code` and unloads.
    pub(crate) async fn stop(&mut self, code: u16, reason: &str) {
        self.snapshot().await;
        self.end(code, reason);
    }

    /// The document was deleted under the room: close everyone and unload.
    pub(crate) fn vanish(&mut self) {
        tracing::info!(room = %self.key, "salon: the {} was deleted, unloading", self.policy.noun);
        self.end(1008, &format!("{} deleted", self.policy.noun));
    }

    /// Unloads without saving: every member is closed with `code` and the room stops at once.
    pub(crate) fn end(&mut self, code: u16, reason: &str) {
        (self.owed.flush_at, self.owed.since, self.owed.unflushed, self.ended) = (None, None, false, true);
        let conns: Vec<u64> = self.conns.keys().copied().collect();
        for conn in conns {
            self.drop_conn(conn, Some(crate::command::Out::Close(code, reason.to_owned())));
        }
    }
}
