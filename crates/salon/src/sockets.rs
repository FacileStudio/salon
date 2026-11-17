//! The live member sockets of a hub, so a shutdown can wait for their close frames.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::watch;

use crate::hooks::Host;
use crate::hub::Hub;

/// Each running socket task by connection id, resolved once the task ends.
#[derive(Clone, Default)]
pub(crate) struct Sockets(Arc<Mutex<HashMap<u64, watch::Receiver<()>>>>);

impl Sockets {
    fn lock(&self) -> MutexGuard<'_, HashMap<u64, watch::Receiver<()>>> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A live socket task: dropping it marks the task done for a shutdown waiting on it.
pub(crate) struct Open {
    sockets: Sockets,
    conn: u64,
    _live: watch::Sender<()>,
}

impl Drop for Open {
    fn drop(&mut self) {
        self.sockets.lock().remove(&self.conn);
    }
}

impl<H: Host> Hub<H> {
    /// Registers the socket task of connection `conn`; it counts as live until the guard drops.
    pub(crate) fn open_socket(&self, conn: u64) -> Open {
        let (live, gone) = watch::channel(());
        self.sockets.lock().insert(conn, gone);
        Open { sockets: self.sockets.clone(), conn, _live: live }
    }

    /// A handle on each socket task of `conns` still running, resolved once it ends.
    pub(crate) fn live_sockets(&self, conns: &[u64]) -> Vec<watch::Receiver<()>> {
        let sockets = self.sockets.lock();
        conns.iter().filter_map(|conn| sockets.get(conn).cloned()).collect()
    }
}
