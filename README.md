# salon

Live co-editing rooms for Facile Suite apps: one Yjs document per open room, shared over
y-websocket, with the app keeping its data and its rules.

## What it does

- Hosts one in-memory Yjs room per open document, loaded on demand and unloaded when idle
- Relays sync, updates and presence, and refuses an update the app's check rejects, exactly
- Keeps members honest: no speaking for another member's presence, viewers stay read-only
- Saves through the app's store on a flush and snapshot policy the app sets
- Runs server-side writes (REST, MCP, AI) inside the room, saved before they answer
- Re-checks each socket and closes it with the app's codes on logout, eviction or shutdown
- Gives browsers one client: reconnect, connection state, terminal stops, custom notices

## Stack

| Layer | Tech |
|---|---|
| Runtime | Rust 1.98 (edition 2024), axum 0.8, tokio 1.54, yrs 0.28.1 |
| Client | TypeScript, y-websocket 3, yjs 13.6 |

## Install

```toml
salon = { git = "https://github.com/FacileStudio/salon", tag = "v0.1.0" }
```

```json
"@facile/salon": "github:FacileStudio/salon#v0.1.0"
```

The app implements `Host` on its state and `Space` for one room's document, then hands each
authorized socket to `salon::accept`:

```rust
async fn upgrade(State(state): State<AppState>, who: Identity, Path(id): Path<Uuid>,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>) -> tronc::Result<Response> {
    let writer = access::check(&state, who.user_id, id, Access::Viewer).await? >= Access::Editor;
    let member = Who { user_id: Some(who.user_id), share: None, name: None, writer };
    Ok(salon::accept(state, id, Caller { who: member, session_id: who.session_id }, upgrade))
}
```

```ts
import { openRoom, socketUrl } from '@facile/salon';

const room = openRoom({ base: socketUrl('/api/documents', location), room: `${id}/ws` }, ydoc, {
	onconnection: (state) => (connection = state),
	onstop: (stop, reason) => showStop(stop, reason)
});
```

## Documentation

| Doc | What's in it |
|---|---|
| [Protocol](spec/PROTOCOL.md) | The wire contract: sync, presence, notices, close codes, client rules |
| [Architecture](docs/architecture.md) | The hub, the room loop, the two hooks and what each owns |
| [Configuration](docs/configuration.md) | Every `Policy` field and the values Docs and Toile use |
| [Development](docs/development.md) | Toolchain, the quality gate, tests, releasing |

---
Part of the [Facile Suite](https://facile.studio) — self-hosted tools for creative studios
and freelancers. One login, zero cloud dependency.
