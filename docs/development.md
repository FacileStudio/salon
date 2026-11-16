# Development

## Toolchain

`rust-toolchain.toml` pins Rust 1.98.1; `mise.toml` pins bun 1.4.2 and Node 24 (vitest's jsdom
needs Node 22 or later).

```sh
mise install
bun install
```

On a shared build host, keep the target directory out of the checkout and drop debug info:

```sh
export CARGO_TARGET_DIR=/srv/scratch/cache/cargo-target-salon
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
```

## The gate

```sh
sh scripts/check.sh
```

It runs rustfmt, clippy with warnings as errors, the Rust tests, the client type check and
tests, then `filet check .`. CI runs the same script, then filet again at its pinned version.

## Tests

- `crates/salon/tests/rooms/` is one test binary. `support/` is a toy app: its documents are a
  Yjs map `m` that refuses the key `bad` and panics on the key `panic`, stored in memory,
  served by axum on a free port. `peer.rs` is a y-websocket client.
- `src/lib/room.test.ts` tests the browser client against a fake provider.

Each test file stays at 8 functions or fewer: filet counts tests toward `funcsPerFile`.

## Releasing

1. Move the `Unreleased` notes under a dated heading in `CHANGELOG.md`, and bump `version` in
   `Cargo.toml` (`workspace.package`) and `package.json` together.
2. Merge to `main`.
3. Tag the merge commit and push the tag: `git tag -a vX.Y.Z <sha> -m vX.Y.Z && git push origin vX.Y.Z`.
4. Apps pin the tag: `salon = { git = "...", tag = "vX.Y.Z" }` and
   `"@facile/salon": "github:FacileStudio/salon#vX.Y.Z"`.

While on `v0`, a breaking change to the Rust API, the client API or [the protocol](../spec/PROTOCOL.md)
bumps the minor.
