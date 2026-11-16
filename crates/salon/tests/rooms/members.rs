use crate::support::peer::Peer;
use crate::support::{serve, toy, value};
use uuid::Uuid;

fn url(addr: std::net::SocketAddr, room: Uuid, user: i64) -> String {
    format!("ws://{addr}/r/{room}/{user}")
}

#[tokio::test]
async fn an_update_reaches_the_other_member_and_is_flushed() {
    let app = toy();
    let (addr, room) = (serve(app.clone()).await, Uuid::new_v4());
    let (mut a, mut b) = (Peer::connect(&url(addr, room, 1)).await, Peer::connect(&url(addr, room, 2)).await);
    a.set("k", "v").await;
    b.pump().await;
    assert_eq!(value(&b.doc, "k").as_deref(), Some("v"));
    let stored = app.store.lock().unwrap().states.get(&room).cloned().unwrap();
    assert!(String::from_utf8_lossy(&stored).contains('v'));
    assert_eq!(app.store.lock().unwrap().saves, [salon::Due::Flush]);
}

#[tokio::test]
async fn a_refused_update_closes_its_sender_and_leaves_the_document_as_it_was() {
    let app = toy();
    let (addr, room) = (serve(app.clone()).await, Uuid::new_v4());
    let (mut a, mut b) = (Peer::connect(&url(addr, room, 1)).await, Peer::connect(&url(addr, room, 2)).await);
    a.set("k", "v").await;
    a.set("bad", "x").await;
    a.pump().await;
    b.pump().await;
    assert_eq!(a.closed, Some((1008, "bad key".to_owned())));
    assert_eq!(value(&b.doc, "bad"), None);
    let read = app.hub.read(&app, room, |pad| (value(&pad.doc, "k"), value(&pad.doc, "bad"))).await.unwrap();
    assert_eq!(read, (Some("v".to_owned()), None));
}

#[tokio::test]
async fn a_viewers_update_is_dropped_but_its_socket_stays() {
    let app = toy();
    let (addr, room) = (serve(app.clone()).await, Uuid::new_v4());
    let (mut viewer, mut writer) = (Peer::connect(&url(addr, room, 0)).await, Peer::connect(&url(addr, room, 1)).await);
    viewer.set("k", "v").await;
    viewer.pump().await;
    writer.pump().await;
    assert_eq!((viewer.closed.clone(), value(&writer.doc, "k")), (None, None));
}

#[tokio::test]
async fn a_member_cannot_speak_for_another_members_presence_and_its_name_is_the_servers() {
    let (addr, room) = (serve(toy()).await, Uuid::new_v4());
    let (mut a, mut b) = (Peer::connect(&url(addr, room, 1)).await, Peer::connect(&url(addr, room, 2)).await);
    a.announce(7, "Owner").await;
    b.pump().await;
    b.announce(7, "Mallory").await;
    b.announce(9, "Owner").await;
    a.pump().await;
    assert_eq!(a.presence, [r#"{"user":{"guest":false,"name":"user2"}}"#]);
    assert_eq!(b.presence, [r#"{"user":{"guest":false,"name":"user1"}}"#]);
}

#[tokio::test]
async fn a_member_announcing_more_than_eight_presence_ids_is_closed() {
    let (addr, room) = (serve(toy()).await, Uuid::new_v4());
    let mut a = Peer::connect(&url(addr, room, 1)).await;
    for id in 1..=9 {
        a.announce(id, "A").await;
    }
    a.pump().await;
    assert_eq!(a.closed.map(|(code, _)| code), Some(1008));
}

#[tokio::test]
async fn a_panic_while_applying_closes_the_sender_with_1011_and_the_room_lives_on() {
    let app = toy();
    let (addr, room) = (serve(app.clone()).await, Uuid::new_v4());
    let mut a = Peer::connect(&url(addr, room, 1)).await;
    a.set("panic", "now").await;
    a.pump().await;
    assert_eq!(a.closed, Some((1011, "update failed".to_owned())));
    let mut b = Peer::connect(&url(addr, room, 2)).await;
    b.set("k", "v").await;
    b.pump().await;
    assert_eq!(b.closed, None);
}
