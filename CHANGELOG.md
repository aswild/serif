# Serif Changelog

## 0.1.1 (2022-11-22)

- **changed**: Use [`tracing-log`] to normalize metadata from events that originate in the [`log`]
  crate, fixing the target and overly verbose fields of these events.

[`tracing-log`]: https://lib.rs/crates/tracing-log
[`log`]: https://lib.rs/crates/log

## 0.1.0 (2022-11-21)

Initial Release
