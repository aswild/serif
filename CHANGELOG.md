# Serif Changelog

## 0.2.1 (2025-09-16)

- **changed**: Update dependencies
  - Bump `nu-ansi-term` to 0.50.0 to match `tracing-subscriber` 0.3.20, removing all duplicates from
    `serif`'s dependency tree.
- The headline change of `tracing-subscriber` 0.3.20 to escape ANSI sequences in logged messages
  is not carried into `serif`, which uses different custom code to format messages.
  - This change in `tracing-susbcriber` has already caused numerous complaints to be raised because
    now ANSI colors don't work either.
  - If a more precise way of stripping potentially harmful ANSI escapes arises while still allowing
    colors and other harmless formatting, I may add it to a future version of `serif`.

## 0.2.0 (2025-06-01)

- **breaking**: Replace `chrono` with `jiff` in `serif::TimeFormat`. This changes the meaning of
  the format strings to be jiff's rather than chrono's.

- **breaking**: Changed methods on `serif::TimeFormat`:
  - Remove `local_const` and `utc_const`
  - Replace `format_timestamp` with `render`
  - Add a new `render_now` method to format the current timestamp
  - `local_custom` and `utc_custom` constructors now may panic when debug assertions are enabled if
    the format string is invalid.

- The MSRV is Rust 1.70.0 to support `std::io::IsTerminal`

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
