use std::time::Duration;

use crate::support::peer::Peer;
use crate::support::{Pad, serve, toy, value};
use salon::{Change, Due, Fault, Kind};
use uuid::Uuid;
use yrs::{Map, Transact};

fn set(pad: &mut Pad, k: &str, v: &str) -> Option<Vec<u8>> {
    let map = pad.doc.get_or_insert_map("m");
    let mut txn = pad.doc.transact_mut();
    map.insert(&mut txn, k, v);
    Some(txn.encode_update_v1())
}

#[tokio::test]
async fn a_server_write_is_relayed_saved_and_answered_after_the_save() {
    let app = toy();
    let (addr, room) = (serve(app.clone()).await, Uuid::new_v4());
    let mut member = Peer::connect(&format!("ws://{addr}/r/{room}/1")).await;
    let edit = |pad: &mut Pad| Ok(set(pad, "k", "server"));
    let answer = |pad: &Pad, saved: bool| (value(&pad.doc, "k"), saved);
    let change = Change { due: Due::Snapshot, author: Some(4), edit, answer };
    let answered = app.hub.write(&app, room, change).await.unwrap();
    member.pump().await;
    assert_eq!(answered, (Some("server".to_owned()), true));
    assert_eq!(value(&member.doc, "k").as_deref(), Some("server"));
    assert_eq!(app.store.lock().unwrap().saves, [Due::Snapshot]);
}

#[tokio::test]
async fn a_refused_write_answers_the_app_error_and_saves_nothing() {
    let app = toy();
    let room = Uuid::new_v4();
    let edit = |_: &mut Pad| Err(Fault::new(Kind::Invalid, "no"));
    let change = Change { due: Due::Flush, author: None, edit, answer: |_: &Pad, _| () };
    let refused = app.hub.write(&app, room, change).await.unwrap_err();
    assert_eq!(refused, Fault::new(Kind::Invalid, "no"));
    assert!(app.store.lock().unwrap().saves.is_empty());
}

#[tokio::test]
async fn a_missing_document_answers_its_load_fault_to_reads_and_writes() {
    let app = toy();
    let room = Uuid::new_v4();
    app.store.lock().unwrap().missing.insert(room);
    let read = app.hub.read(&app, room, |_| ()).await.unwrap_err();
    let change =
        Change { due: Due::Flush, author: None, edit: |pad: &mut Pad| Ok(set(pad, "k", "v")), answer: |_: &Pad, _| () };
    let written = app.hub.write(&app, room, change).await.unwrap_err();
    assert_eq!((read.kind, written.kind), (Kind::NotFound, Kind::NotFound));
}

#[tokio::test]
async fn close_snapshots_then_closes_its_members_with_the_code_given() {
    let app = toy();
    let (addr, room) = (serve(app.clone()).await, Uuid::new_v4());
    let mut member = Peer::connect(&format!("ws://{addr}/r/{room}/1")).await;
    member.set("k", "v").await;
    member.pump().await;
    app.hub.close(&[room], 1001, "server restarting").await;
    member.pump().await;
    assert_eq!(member.closed, Some((1001, "server restarting".to_owned())));
    assert_eq!(app.store.lock().unwrap().saves, [Due::Flush, Due::Snapshot]);
}

#[tokio::test]
async fn an_emptied_room_snapshots_then_unloads_once_idle() {
    let app = toy();
    let (addr, room) = (serve(app.clone()).await, Uuid::new_v4());
    let mut member = Peer::connect(&format!("ws://{addr}/r/{room}/1")).await;
    member.set("k", "v").await;
    member.pump().await;
    drop(member);
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(app.store.lock().unwrap().saves, [Due::Flush, Due::Snapshot]);
    let k = app.hub.read(&app, room, |pad| value(&pad.doc, "k")).await.unwrap();
    assert_eq!((k.as_deref(), app.store.lock().unwrap().opens), (Some("v"), 2));
}

#[test]
fn a_long_close_reason_is_cut_on_a_character_boundary() {
    let axum::extract::ws::Message::Close(Some(frame)) = salon::close(1008, "é".repeat(100)) else { panic!() };
    assert_eq!((frame.code, frame.reason.len()), (1008, 122));
}

#[test]
fn presence_passes_unchanged_when_the_app_names_nobody() {
    let who = salon::Who { user_id: None, share: None, name: None, writer: true };
    assert_eq!(&*salon::stamp(r#"{"user":{"name":"Ana"}}"#, &who), r#"{"user":{"name":"Ana"}}"#);
    let named = salon::Who { name: Some("Bo".to_owned()), ..who };
    assert_eq!(&*salon::stamp(r#"{"user":{"name":"Ana"}}"#, &named), r#"{"user":{"guest":true,"name":"Bo"}}"#);
}
