# Changelog

All notable changes to `salon` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow semver: while on
`v0`, a breaking change bumps the minor.

## [Unreleased]

## [0.1.1] - 2026-11-17

### Fixed

- `Hub::shutdown` returns only once every member it closed has had its close frame written, or
  after `SHUTDOWN_WAIT` (2 s). An app that exits right after shutdown no longer drops the
  frames, so members see the code the app chose instead of `1006`.

## [0.1.0] - 2026-11-16

### Added

- The room engine, from the rooms Docs and Toile each carried: the hub, the room loop, sync,
  presence, slow members, socket pumping, re-checks, close and shutdown.
- The `Host` and `Space` hooks: the app keeps its store, its checks, its sign-in and its
  close codes.
- Server-side reads and writes (`Hub::read`, `Hub::write` with a `Change`), retried when a
  room unloads under them.
- The browser client `@facile/salon`: `openRoom`, `stopOf`, `connectionOf`, `socketUrl`.
- The protocol, version 1, in `spec/PROTOCOL.md`: what Docs and Toile already did on the wire.
