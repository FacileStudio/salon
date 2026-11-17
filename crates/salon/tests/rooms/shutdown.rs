use std::net::SocketAddr;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use futures_util::future::join_all;
use salon::SHUTDOWN_WAIT;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::support::peer::Peer;
use crate::support::{Toy, serve, toy};

/// Members of one room: more than the 61 tasks tokio's current-thread scheduler runs before it
/// polls `block_on` again, so a shutdown that does not wait returns before some sockets ran.
const MEMBERS: i64 = 100;

/// Serves `app` on a current-thread runtime of its own thread. Sending on the returned channel
/// shuts the hub down with 1013, then drops the runtime as an exiting process would; the thread
/// answers how long shutdown took.
fn server(app: Toy) -> (SocketAddr, oneshot::Sender<()>, JoinHandle<Duration>) {
    let (ready, addr) = std::sync::mpsc::channel();
    let (go, stop) = oneshot::channel();
    let thread = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let took = runtime.block_on(async move {
            ready.send(serve(app.clone()).await).unwrap();
            _ = stop.await;
            let started = Instant::now();
            app.hub.shutdown(1013, "server restarting").await;
            started.elapsed()
        });
        drop(runtime);
        took
    });
    (addr.recv().unwrap(), go, thread)
}

async fn stopped(go: oneshot::Sender<()>, thread: JoinHandle<Duration>) -> Duration {
    go.send(()).unwrap();
    tokio::task::spawn_blocking(move || thread.join().unwrap()).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_returns_only_after_every_member_got_its_close_frame() {
    let (addr, go, thread) = server(toy());
    let room = Uuid::new_v4();
    let joins = (1..=MEMBERS).map(|user| async move { Peer::connect(&format!("ws://{addr}/r/{room}/{user}")).await });
    let mut members = join_all(joins).await;
    stopped(go, thread).await;
    for member in &mut members {
        member.pump().await;
        assert_eq!(member.closed, Some((1013, "server restarting".to_owned())));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_member_that_stopped_reading_does_not_hold_shutdown_past_its_wait() {
    let (addr, go, thread) = server(toy());
    let room = Uuid::new_v4();
    let _stalled = Peer::connect(&format!("ws://{addr}/r/{room}/1")).await;
    let mut writer = Peer::connect(&format!("ws://{addr}/r/{room}/2")).await;
    for round in 0..32 {
        writer.set("k", &round.to_string().repeat(1 << 20)).await;
    }
    writer.pump().await;
    let took = stopped(go, thread).await;
    assert!(took >= SHUTDOWN_WAIT && took < SHUTDOWN_WAIT + Duration::from_secs(1), "shutdown took {took:?}");
}
