//! Commands a room serves: members joining, speaking and leaving, notices and evictions.

use tokio::time::Instant;

use crate::command::{Command, Member, Out};
use crate::hooks::Host;
use crate::presence::departure;
use crate::room::{Conn, Room};
use crate::space::Space;
use crate::sync::{Effect, guarded, opening};

impl<H: Host> Room<H> {
    /// Handles one command; `false` ends the room.
    pub(crate) async fn handle(&mut self, command: Command<H::Space>) -> bool {
        match command {
            Command::Join(member, reply) => {
                self.join(member);
                _ = reply.send(Ok(()));
            }
            Command::Message(conn, frame) => self.message(conn, frame).await,
            Command::Leave(conn) => self.leave(conn).await,
            Command::Read(read) => read(Ok(&self.space)),
            Command::Write(job) => self.write(job).await,
            Command::Broadcast(frame) => self.broadcast(None, &frame),
            Command::Evict(users) => self.evict(&users).await,
            Command::Revoke(share) => self.close_where(|conn| conn.who.share == Some(share)),
            Command::Shutdown(code, reason, done) => {
                self.stop(code, &reason).await;
                _ = done.send(());
                return false;
            }
        }
        true
    }

    fn join(&mut self, member: Member) {
        let mut who = member.who;
        who.writer &= self.space.writable();
        self.conns.insert(member.conn, Conn { outbox: member.outbox, who, clients: Default::default() });
        self.empty_since = None;
        for frame in opening(&self.space, &self.awareness) {
            self.send(member.conn, Out::Binary(frame));
        }
    }

    async fn message(&mut self, conn: u64, frame: Vec<u8>) {
        let Some(who) = self.conns.get(&conn).map(|c| c.who.clone()) else { return };
        let writer = who.writer;
        let Some(effects) = self.lend(move |space, awareness| guarded(space, awareness, &frame, writer)).await else {
            tracing::error!(room = %self.key, "salon: a frame task did not finish, unloading without saving");
            return self.end(1011, "room failed");
        };
        for effect in effects {
            match effect {
                Effect::Reply(bytes) => self.send(conn, Out::Binary(bytes)),
                Effect::Accepted(bytes, len) => {
                    self.broadcast(Some(conn), &bytes);
                    self.accepted(len, who.user_id);
                }
                Effect::Presence(update) => self.presence(conn, update),
                Effect::Refuse(code, reason) => self.drop_conn(conn, Some(Out::Close(code, reason))),
            }
        }
    }

    async fn leave(&mut self, conn: u64) {
        self.drop_conn(conn, None);
        if self.conns.is_empty() {
            self.snapshot().await;
        }
    }

    /// Removes a member, tells the others its cursors left, and closes it with `close`.
    pub(crate) fn drop_conn(&mut self, conn: u64, close: Option<Out>) {
        let Some(member) = self.conns.remove(&conn) else { return };
        if let Some(close) = close {
            _ = member.outbox.try_send(close);
        }
        if let Some(gone) = departure(&mut self.awareness, &member.clients) {
            self.broadcast(None, &gone);
        }
        if self.conns.is_empty() {
            self.empty_since = Some(Instant::now());
        }
    }

    /// Sends `frame` to every member but `except`.
    pub(crate) fn broadcast(&mut self, except: Option<u64>, frame: &[u8]) {
        let targets: Vec<u64> = self.conns.keys().copied().filter(|c| Some(*c) != except).collect();
        for conn in targets {
            self.send(conn, Out::Binary(frame.to_vec()));
        }
    }

    async fn evict(&mut self, users: &[i64]) {
        let mut lost = Vec::new();
        for user in users.iter().filter(|u| self.conns.values().any(|c| c.who.user_id == Some(**u))) {
            if self.host.lost(self.key, *user).await {
                lost.push(*user);
            }
        }
        self.close_where(|conn| conn.who.user_id.is_some_and(|user| lost.contains(&user)));
    }

    /// Closes every member `pick` selects with 1008: its access was revoked.
    fn close_where(&mut self, pick: impl Fn(&Conn) -> bool) {
        let gone: Vec<u64> = self.conns.iter().filter(|(_, c)| pick(c)).map(|(id, _)| *id).collect();
        for conn in gone {
            self.drop_conn(conn, Some(Out::Close(1008, "access revoked".to_owned())));
        }
    }
}
