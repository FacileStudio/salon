# Architecture

salon splits a live room into the part every app shares and the part only the app knows.

## What salon owns

| Piece | File | Job |
|---|---|---|
| Hub | `hub.rs`, `calls.rs` | Every open room of one app, keyed by document id. Loads a room on the first command, retries a command that reached a room as it unloaded. |
| Room loop | `room.rs` | One task per open document. Serves commands, wakes for the next flush, snapshot or idle deadline. |
| Sync | `sync.rs` | y-websocket messages: sync steps, updates, awareness, custom notices. Catches a panic and asks the app to recover. |
| Presence | `presence.rs` | Claims, the 8-id cap, and the server-written name. |
| Members | `members.rs` | Join, leave, relay, evict, revoke. A full outbox closes a member with `1013`. |
| Saving | `persist.rs` | When to call the app's store: flush, snapshot, on empty, on stop. |
| Writes | `writes.rs` | Server-side reads and writes, run inside the room and saved before the answer. |
| Socket | `ws.rs` | The upgrade limits, the frame pump, the re-check timer, the close reason cut. |
| Blocking | `lend.rs` | Runs document work on a blocking thread when the app's check is heavy. |

salon depends on axum, tokio, yrs and serde_json. It never depends on tronc, porte or sqlx:
the app owns its errors, its sign-in and its database.

## What the app owns

`Host` is implemented on the app state:

| Method | The app answers |
|---|---|
| `rooms` | Its `Hub` and its `Policy`. |
| `fault` | Its own error for a salon `Fault`. |
| `open` | The document of a room, or a fault (missing, unreadable). |
| `save` | Whether a flush or a snapshot is stored, owed, to retry, or the document is gone. |
| `alive` | Whether a socket may stay, re-checked on each `Policy::recheck`. |
| `lost` | Whether a user lost access, for an eviction. |

`Space` is one room's document:

| Method | The app answers |
|---|---|
| `doc` | The live Yjs document. |
| `apply` | A member's update, kept only when the document still validates. A refusal leaves the document exactly as it was. |
| `recover` | A valid document again after a panic. Default: nothing to do. |
| `writable` | Whether members may write. Default: yes. |

## A member's update, end to end

1. `ws.rs` reads a binary frame and sends it to the room.
2. `members.rs` hands it to `sync.rs`, on a blocking thread when the policy says so.
3. `sync.rs` decodes each message. An update goes to `Space::apply`.
4. Accepted: the room relays it to the other members and records what it owes the store.
   Refused: the sender is closed with the app's code; nobody else sees the update.
5. `persist.rs` calls `Host::save` with `Due::Flush` once `Policy::flush` has passed, and with
   `Due::Snapshot` past a threshold, when the room empties, and when it stops.

## A server-side write

`Hub::write` sends a `Change` to the room: `edit` runs on the document and answers the update
to relay; `answer` reads the document after the save. A room that unloads first sends the
change back, and the hub sends it again, three times at most.
