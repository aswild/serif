# Serif Changelog

## 0.1.5 (2024-06-21)

- Update dependency `tracing-log` dependency to the latest version. No functional changes.

## 0.1.4 (2023-03-20)

- Update dependency requirements for `tracing` crates. No functional changes, and no changes at all
  for anyone who's run a normal `cargo update` command.

## 0.1.3 (2023-03-09)

- **changed**: Replace [`atty`] with [`is-terminal`] for terminal detection.

[`atty`]: https://lib.rs/crates/atty
[`is-terminal`]: https://lib.rs/crates/is-terminal

## 0.1.2 (2023-01-19)

- **added**: Include more macros from tracing in `serif::macros`, namely `debug_span!` and friends
  for other levels, as well as `event!` and `event_enabled!`.

## 0.1.1 (2022-11-22)

- **changed**: Use [`tracing-log`] to normalize metadata from events that originate in the [`log`]
  crate, fixing the target and overly verbose fields of these events.

[`tracing-log`]: https://lib.rs/crates/tracing-log
[`log`]: https://lib.rs/crates/log

## 0.1.0 (2022-11-21)

Initial Release
