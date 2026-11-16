//! One member socket: joins its room, pumps frames both ways, re-checks its caller, leaves.

use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::Instant;
use uuid::Uuid;

use crate::command::{Command, Member, Out, Who};
use crate::hooks::Host;

/// The largest frame a member may send.
pub const MAX_FRAME: usize = 2 << 20;
/// How many frames wait for a slow member before it is closed with 1013.
pub const OUTBOX: usize = 512;
/// The longest close reason a WebSocket close frame carries, in bytes (RFC 6455 section 5.5).
pub const MAX_REASON: usize = 123;

/// A close frame whose reason is cut to [`MAX_REASON`] bytes on a character boundary: a longer
/// one fails to send and the client sees 1006 instead of the code.
pub fn close(code: u16, mut reason: String) -> Message {
    let cut = (0..=MAX_REASON.min(reason.len())).rev().find(|&at| reason.is_char_boundary(at)).unwrap_or(0);
    reason.truncate(cut);
    Message::Close(Some(CloseFrame { code, reason: reason.into() }))
}

/// Who a socket speaks for, as the app authorized it at the upgrade.
#[derive(Debug, Clone)]
pub struct Caller {
    /// The member it joins as.
    pub who: Who,
    /// The session row behind a member's cookie; zero on a bearer token and for guests.
    pub session_id: i64,
}

/// Upgrades `upgrade` into a member socket of room `key`; a request that is not a WebSocket
/// upgrade gets axum's rejection. The app authorizes `caller` before calling this.
pub fn accept<H: Host>(
    host: H,
    key: Uuid,
    caller: Caller,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    match upgrade {
        Ok(upgrade) => upgrade
            .max_message_size(MAX_FRAME + (1 << 20))
            .max_frame_size(MAX_FRAME + (1 << 20))
            .on_upgrade(move |socket| serve(socket, host, key, caller)),
        Err(rejection) => rejection.into_response(),
    }
}

async fn serve<H: Host>(socket: WebSocket, host: H, key: Uuid, caller: Caller) {
    let (mut sink, stream) = socket.split();
    let (outbox, inbox) = mpsc::channel(OUTBOX);
    let hub = host.rooms().0.clone();
    let conn = hub.next_id();
    let member = Member { conn, who: caller.who.clone(), outbox };
    if let Err(fault) = hub.ask(&host, key, |reply| Command::Join(member.clone(), reply)).await {
        _ = sink.send(close(1011, fault.message)).await;
        return;
    }
    pump(&host, key, conn, &caller, (sink, stream, inbox)).await;
    hub.tell(key, Command::Leave(conn)).await;
}

type Halves = (SplitSink<WebSocket, Message>, SplitStream<WebSocket>, mpsc::Receiver<Out>);

async fn pump<H: Host>(host: &H, key: Uuid, conn: u64, caller: &Caller, (mut sink, mut stream, mut inbox): Halves) {
    let (hub, policy) = host.rooms();
    let opened = Instant::now();
    let mut recheck = tokio::time::interval_at(opened + policy.recheck, policy.recheck);
    loop {
        tokio::select! {
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Binary(frame))) if frame.len() > MAX_FRAME => {
                    _ = sink.send(close(1009, "frame over 2 MiB".to_owned())).await;
                    break;
                }
                Some(Ok(Message::Binary(frame))) => hub.tell(key, Command::Message(conn, frame.to_vec())).await,
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
            out = inbox.recv() => match out {
                Some(Out::Binary(frame)) => if sink.send(Message::Binary(frame.into())).await.is_err() { break },
                Some(Out::Close(code, reason)) => { _ = sink.send(close(code, reason)).await; break }
                None => break,
            },
            _ = recheck.tick() => if let Some((code, reason)) = host.alive(caller, opened).await {
                _ = sink.send(close(code, reason)).await;
                break;
            },
        }
    }
}
