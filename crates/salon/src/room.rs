//! The room task: one per open document, serving commands until it is idle or shut down.

use std::collections::{HashMap, HashSet};

use tokio::sync::mpsc;
use tokio::time::Instant;
use uuid::Uuid;
use yrs::ClientID;
use yrs::sync::Awareness;

use crate::command::{Command, Out, Who};
use crate::drain::{answer_gone, refuse_all};
use crate::hooks::{Host, Policy};

/// One member's connection inside a room.
pub(crate) struct Conn {
    pub(crate) outbox: mpsc::Sender<Out>,
    pub(crate) who: Who,
    pub(crate) clients: HashSet<ClientID>,
}

/// What the room owes its store.
#[derive(Debug, Default)]
pub(crate) struct Owed {
    /// Updates were accepted since the last save.
    pub(crate) unflushed: bool,
    /// Updates accepted since the last snapshot, and their weight.
    pub(crate) count: usize,
    pub(crate) bytes: usize,
    /// When the first update since the last snapshot was accepted.
    pub(crate) since: Option<Instant>,
    /// When the next flush is due.
    pub(crate) flush_at: Option<Instant>,
    /// The last member who changed the document.
    pub(crate) author: Option<i64>,
}

/// A loaded room.
pub(crate) struct Room<H: Host> {
    pub(crate) host: H,
    pub(crate) key: Uuid,
    pub(crate) policy: Policy,
    pub(crate) space: H::Space,
    pub(crate) awareness: Awareness,
    pub(crate) conns: HashMap<u64, Conn>,
    pub(crate) owed: Owed,
    pub(crate) empty_since: Option<Instant>,
    pub(crate) ended: bool,
}

/// The room task: loads the document, then serves commands until it has been empty with
/// nothing owed for [`Policy::idle`], or is shut down.
pub(crate) async fn run<H: Host>(host: H, key: Uuid, generation: u64, mut commands: mpsc::Receiver<Command<H::Space>>) {
    let mut room = match host.open(key).await {
        Ok(space) => Room::new(host, key, space),
        Err(fault) => return refuse_all(&host, key, generation, commands, fault).await,
    };
    loop {
        let deadline = room.deadline();
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => if !room.handle(command).await { break },
                None => break,
            },
            () = sleep_until(deadline) => if room.tick().await { break },
        }
    }
    room.host.rooms().0.forget(key, generation);
    commands.close();
    while let Ok(command) = commands.try_recv() {
        answer_gone(command);
    }
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

impl<H: Host> Room<H> {
    fn new(host: H, key: Uuid, space: H::Space) -> Self {
        let policy = host.rooms().1;
        let (awareness, conns) = (Awareness::new(yrs::Doc::new()), HashMap::new());
        let (owed, empty_since, ended) = (Owed::default(), Some(Instant::now()), false);
        Room { host, key, policy, space, awareness, conns, owed, empty_since, ended }
    }

    fn deadline(&self) -> Option<Instant> {
        if self.ended {
            return Some(Instant::now());
        }
        let snapshot = self.owed.since.zip(self.policy.snapshot_after).map(|(at, after)| at + after);
        let idle = self.empty_since.filter(|_| !self.owes()).map(|at| at + self.policy.idle);
        [self.owed.flush_at, snapshot, idle].into_iter().flatten().min()
    }

    async fn tick(&mut self) -> bool {
        let now = Instant::now();
        if self.snapshot_due(now) {
            self.snapshot().await;
        } else if self.owed.flush_at.is_some_and(|at| at <= now) {
            self.flush().await;
        }
        self.ended || !self.owes() && self.empty_since.is_some_and(|at| at + self.policy.idle <= now)
    }

    /// Whether the store is owed anything.
    pub(crate) fn owes(&self) -> bool {
        self.owed.unflushed || self.owed.since.is_some()
    }

    /// Queues `out` for `conn`; a member whose outbox is full is dropped with 1013.
    pub(crate) fn send(&mut self, conn: u64, out: Out) {
        let full = self.conns.get(&conn).is_some_and(|c| c.outbox.try_send(out).is_err());
        if full && let Some(conn) = self.conns.remove(&conn) {
            _ = conn.outbox.try_send(Out::Close(1013, "too slow".to_owned()));
        }
    }

    /// Records an accepted update of `len` bytes by `author`; it is flushed after
    /// [`Policy::flush`].
    pub(crate) fn accepted(&mut self, len: usize, author: Option<i64>) {
        let now = Instant::now();
        let owed = &mut self.owed;
        owed.unflushed = true;
        (owed.count, owed.bytes) = (owed.count + 1, owed.bytes + len);
        owed.since.get_or_insert(now);
        owed.author = author.or(owed.author);
        match self.policy.debounce {
            true => owed.flush_at = Some(now + self.policy.flush),
            false => _ = owed.flush_at.get_or_insert(now + self.policy.flush),
        }
    }
}
