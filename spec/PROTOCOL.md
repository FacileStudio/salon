# salon protocol, version 1

Normative. This is the wire contract between a salon room and its browser clients. Version 1
writes down what Docs (v0.13.3) and Toile (v0.18.1) already did on 2026-11-16, so moving either
app onto salon changes nothing a client sees. A change to a message, a close code or a limit
is a new version.

## 1. Transport

1. A room is one WebSocket per member, at a route the app chooses (Docs:
   `/api/documents/{id}/ws`, Toile: `/api/boards/{id}/ws` and `/api/share/{token}/ws`).
2. The app authorizes the upgrade (session cookie and Origin check, a bearer token, or a share
   link) before salon sees the socket. salon never reads a cookie or a token.
3. Frames are binary and follow y-websocket 3: each frame holds one or more y-protocols
   messages. Types: `0` sync, `1` awareness, `2` auth (ignored), `3` awareness query, `100`
   custom notice. Text frames are ignored.
4. A frame over 2 MiB closes the socket with `1009` `frame over 2 MiB`.

## 2. Sync

1. On join, the server sends sync step 1 (its state vector), then one awareness message with
   every present member, when there is one.
2. A sync step 1 from the client is answered with sync step 2.
3. A sync step 2 or an update from a **writer** is applied. When it holds anything, it is
   relayed as an update to every other member. An empty update is not relayed.
4. A sync step 2 or an update from a **viewer** is dropped without a reply; the socket stays
   open. A member is a viewer when the app says so or when the document refuses writes (Docs:
   a page in the trash).
5. An update the app's check refuses closes its sender with `1008` and the app's reason. The
   document is back to exactly the state before the update, and no other member sees it. The
   app may refuse with `1009` instead (Docs: a page over 5 MiB).
6. An undecodable update or a malformed frame closes its sender with `1008`.
7. A failure inside the server while it applies an update closes the sender with `1011`
   `update failed` (the document was restored) or `room failed` (it could not be).

## 3. Presence

1. Presence is relayed to the other members and never stored.
2. A connection may speak only for Yjs client ids that no other connection of the room
   claimed. Entries for another connection's ids are dropped without a reply.
3. A connection that claims more than 8 ids is closed with `1008`
   `more than 8 presence clients`.
4. When the app names the member (Toile), the server writes `user.name` and `user.guest` into
   each of its presence entries, over whatever the client sent. When it does not (Docs), the
   entries pass unchanged.
5. When a member leaves, the others receive the removal of every id it claimed.

## 4. Custom notices

1. A custom notice is message type `100` carrying one varstring: a JSON object with a `type`
   key. The app defines the types (Toile: `{"type":"comments-changed"}`).
2. A client that cannot parse a notice treats it as a notice with no type.

## 5. Closing

1. A close reason is at most 123 bytes, cut on a character boundary.
2. A member whose 512-frame outbox is full is closed with `1013` `too slow`.
3. Every re-check interval (30 s in production) the app re-checks the caller. It closes the
   socket with its own code: `1008` when the session or share link ended, `1001`
   `token age, reconnect` when a bearer token is older than 15 minutes.
4. A member who lost access to the document is closed with `1008` `access revoked`.
5. A document deleted under an open room closes every member with `1008` `<noun> deleted`
   (`board deleted`, `document deleted`).
6. When the app closes rooms or stops, it picks the code: Docs closes with `1001`
   `server restarting` in both cases; Toile closes a deleted board with `1008`
   `board deleted` and stops with `1013` `server restarting`.
7. A room that cannot load its document closes the joining socket with `1011` and the load
   error.

## 6. Clients

1. A client stops for good on `1008`, `1009` and `1011`, and shows why.
2. It reconnects with backoff on every other close, `1001`, `1013` and a dropped network
   (`1006`) included.
3. `1001` tells the page its room closed (Docs reloads its view of the document's state).

## 7. Server writes

1. A change from the server side (REST, MCP, AI, an import) runs inside the room when the room
   is open, so members see it live.
2. It is checked like a member's update. A refused change changes nothing and answers the
   app's error.
3. It is relayed to every member and saved before its answer.

## 8. Reserved: channels

Live updates without a shared document (a timer started, a mail arrived) will use the same
transport, closing and client rules, with their own message types. Version 1 defines none and
ships no channel code: channels arrive with their first app.

## Sources and checks

| Rule | Toile v0.18.1 | Docs v0.13.3 | salon test |
|---|---|---|---|
| 1.4 | `rooms/ws.rs` `MAX_FRAME` | `rooms/ws.rs` `MAX_FRAME` | covered by `ws.rs`, not by a test |
| 2.3 | `rooms/sync.rs` `on_update` | `rooms/sync.rs` `on_update` | `members::an_update_reaches_the_other_member_and_is_flushed` |
| 2.4 | `rooms/sync.rs` writer flag | `rooms/members.rs` `join` (trash) | `members::a_viewers_update_is_dropped_but_its_socket_stays` |
| 2.5 | `rooms/core.rs` `settle` | `rooms/sync.rs` `apply_checked` | `members::a_refused_update_closes_its_sender_and_leaves_the_document_as_it_was` |
| 2.7 | `rooms/sync.rs` `guarded` | `rooms/blocking.rs` `handle_owned` | `members::a_panic_while_applying_closes_the_sender_with_1011_and_the_room_lives_on` |
| 3.2, 3.4 | `rooms/presence.rs` | `rooms/clients.rs` | `members::a_member_cannot_speak_for_another_members_presence_and_its_name_is_the_servers`, `server::presence_passes_unchanged_when_the_app_names_nobody` |
| 3.3 | `rooms/presence.rs` `MAX_CLIENTS` | none (new for Docs) | `members::a_member_announcing_more_than_eight_presence_ids_is_closed` |
| 5.1 | `rooms/ws.rs` `close` | `rooms/ws.rs` `close` | `server::a_long_close_reason_is_cut_on_a_character_boundary` |
| 5.6 | `rooms/drain.rs` | `rooms/hub.rs` `close` | `server::close_snapshots_then_closes_its_members_with_the_code_given` |
| 5.7, 7.2 | `rooms/drain.rs` `refuse_all` | `rooms/drain.rs` `refuse_all` | `server::a_missing_document_answers_its_load_fault_to_reads_and_writes`, `server::a_refused_write_answers_the_app_error_and_saves_nothing` |
| 6.1 to 6.3 | `client/src/lib/board/provider.ts` | `client/src/lib/collab.ts` | `src/lib/room.test.ts` |
| 7.1, 7.3 | `rooms/writes.rs` | `rooms/writes.rs` | `server::a_server_write_is_relayed_saved_and_answered_after_the_save` |

Rule 3.3 is the one rule Docs did not enforce before. A Docs tab announces one presence id,
so no Docs client is affected.
