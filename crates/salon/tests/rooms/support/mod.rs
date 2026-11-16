//! A toy app for the tests: documents are a Yjs map `m` that refuses the key `bad`, stored in
//! memory, served over a real socket.

pub mod app;
pub mod peer;

pub use app::serve;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use salon::{Due, Hub, Policy, Refusal, Space};
use uuid::Uuid;
use yrs::updates::decoder::Decode;
use yrs::{Doc, Map, ReadTxn, StateVector, Transact, Update};

/// What the toy store saw.
#[derive(Default)]
pub struct Store {
    pub states: HashMap<Uuid, Vec<u8>>,
    pub saves: Vec<Due>,
    pub opens: usize,
    pub missing: HashSet<Uuid>,
}

/// The toy app state.
#[derive(Clone)]
pub struct Toy {
    pub hub: Hub<Toy>,
    pub store: Arc<Mutex<Store>>,
    pub policy: Policy,
}

/// The toy document, and its last valid state for [`Space::recover`].
#[derive(Default)]
pub struct Pad {
    pub doc: Doc,
    pub good: Vec<u8>,
}

pub(crate) fn doc_of(state: &[u8]) -> Doc {
    let doc = Doc::new();
    if !state.is_empty() {
        doc.transact_mut().apply_update(Update::decode_v1(state).unwrap()).unwrap();
    }
    doc
}

/// The whole state of `doc`.
pub fn state(doc: &Doc) -> Vec<u8> {
    doc.transact().encode_state_as_update_v1(&StateVector::default())
}

/// The value of key `k` in map `m`.
pub fn value(doc: &Doc, k: &str) -> Option<String> {
    doc.get_or_insert_map("m").get(&doc.transact(), k).map(|v| v.to_string(&doc.transact()))
}

impl Space for Pad {
    fn doc(&self) -> &Doc {
        &self.doc
    }

    fn apply(&mut self, update: &[u8]) -> Result<bool, Refusal> {
        let before = state(&self.doc);
        let decoded = Update::decode_v1(update).map_err(|e| Refusal(1008, e.to_string()))?;
        if decoded.is_empty() {
            return Ok(false);
        }
        self.doc.transact_mut().apply_update(decoded).map_err(|e| Refusal(1008, e.to_string()))?;
        if value(&self.doc, "panic").is_some() {
            panic!("asked to");
        }
        match value(&self.doc, "bad") {
            Some(_) => {
                self.doc = doc_of(&before);
                Err(Refusal(1008, "bad key".to_owned()))
            }
            None => {
                self.good = state(&self.doc);
                Ok(true)
            }
        }
    }

    fn recover(&mut self) -> Result<(), Refusal> {
        self.doc = doc_of(&self.good);
        Ok(())
    }
}

/// A toy app with short timings.
pub fn toy() -> Toy {
    let policy = Policy {
        flush: Duration::from_millis(20),
        debounce: false,
        retry: Duration::from_millis(50),
        snapshot_every: 1000,
        snapshot_bytes: 1 << 20,
        snapshot_after: None,
        idle: Duration::from_millis(100),
        recheck: Duration::from_secs(60),
        blocking: false,
        noun: "pad",
    };
    Toy { hub: Hub::default(), store: Arc::default(), policy }
}
