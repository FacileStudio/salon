# Configuration

salon reads no environment variable. The app passes one `Policy` from `Host::rooms`.

| Field | What it decides | Docs | Toile |
|---|---|---|---|
| `flush` | How long accepted updates wait before a flush | 2 s | 250 ms |
| `debounce` | Whether each update pushes the flush back | yes | no |
| `retry` | How long after a failed save the room tries again | 2 s | 1 s |
| `snapshot_every` | Updates since the last snapshot that make one due | never | 200 |
| `snapshot_bytes` | Weight since the last snapshot that makes one due | never | 4 MiB |
| `snapshot_after` | Age of the oldest update that makes a snapshot due | never | 60 s |
| `idle` | How long an empty room with nothing owed stays loaded | 60 s | 60 s |
| `recheck` | How often a socket re-checks its caller | 30 s | 30 s |
| `blocking` | Whether document work runs on a blocking thread | yes | no |
| `noun` | The document word in messages | `document` | `board` |

"never" is `usize::MAX` for the counts and `None` for `snapshot_after`.

## Fixed limits

| Limit | Value | Where |
|---|---|---|
| Largest member frame | 2 MiB | `MAX_FRAME` |
| Frames queued for one member | 512 | `OUTBOX` |
| Presence ids per connection | 8 | `MAX_CLIENTS` |
| Close reason | 123 bytes | `MAX_REASON` |
| Commands queued for one room | 1024 | `hub::QUEUE` |
| Tries for a command whose room was unloading | 3 | `calls.rs`, `writes.rs` |

A change to one of these limits is a protocol change: see [the protocol](../spec/PROTOCOL.md).
