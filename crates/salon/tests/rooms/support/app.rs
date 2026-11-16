//! The toy app's hooks and its socket server.

use std::net::SocketAddr;

use axum::Router;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::{Path, State};
use axum::response::Response;
use axum::routing::get;
use salon::{Caller, Due, Fault, Host, Hub, Kind, Policy, Saved, Who};
use tokio::time::Instant;
use uuid::Uuid;

use super::{Pad, Toy, doc_of, state};

impl Host for Toy {
    type Space = Pad;
    type Error = Fault;

    fn rooms(&self) -> (&Hub<Self>, Policy) {
        (&self.hub, self.policy)
    }

    fn fault(fault: Fault) -> Fault {
        fault
    }

    async fn open(&self, key: Uuid) -> Result<Pad, Fault> {
        let mut store = self.store.lock().unwrap();
        store.opens += 1;
        if store.missing.contains(&key) {
            return Err(Fault::new(Kind::NotFound, "pad not found"));
        }
        let good = store.states.get(&key).cloned().unwrap_or_default();
        Ok(Pad { doc: doc_of(&good), good })
    }

    async fn save(&self, key: Uuid, space: &mut Pad, due: Due, _author: Option<i64>) -> Saved {
        let mut store = self.store.lock().unwrap();
        store.saves.push(due);
        store.states.insert(key, state(&space.doc));
        match due {
            Due::Flush => Saved::Flushed,
            Due::Snapshot => Saved::Stored,
        }
    }

    async fn alive(&self, _caller: &Caller, _opened: Instant) -> Option<(u16, String)> {
        None
    }

    async fn lost(&self, _key: Uuid, _user: i64) -> bool {
        true
    }
}

async fn upgrade(
    State(toy): State<Toy>,
    Path((room, user)): Path<(String, i64)>,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    let who = Who { user_id: Some(user), share: None, name: Some(format!("user{user}")), writer: user > 0 };
    salon::accept(toy, Uuid::parse_str(&room).unwrap(), Caller { who, session_id: 0 }, upgrade)
}

/// Serves `toy` on a free port: `ws://<addr>/r/<room>/<user>`, a user of 0 or less is a viewer.
pub async fn serve(toy: Toy) -> SocketAddr {
    let app = Router::new().route("/r/{room}/{user}", get(upgrade)).with_state(toy);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}
