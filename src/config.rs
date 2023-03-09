// Copyright 2022 Allen Wild
// SPDX-License-Identifier: Apache-2.0
//! Implementation of `serif::Config`. This module is private, but its pub types are exported and
//! inlined at the top-level of the `serif` crate.

use std::env::{self, VarError};
use std::io;

use is_terminal::IsTerminal;
use tracing_subscriber::filter::{Directive, EnvFilter, LevelFilter};

use crate::{EventFormatter, FieldFormatter, TimeFormat};

/// The destination for where serif will write logs.
///
/// Only stdout and stderr are supported, due to type system limitations and how [`FmtSubscriber`]
/// is generic over its Writer type.
///
/// [`FmtSubscriber`]: tracing_subscriber::fmt::Subscriber
#[derive(Debug, Clone, Copy)]
pub enum Output {
    /// Log to standard output. This is the default.
    Stdout,
    /// Log to standard error.
    Stderr,
}

impl Default for Output {
    /// The default output destination is stdout
    fn default() -> Self {
        Self::Stdout
    }
}

impl Output {
    /// Is this output stream a terminal?
    ///
    /// This is effectively `impl IsTerminal for Output` but keeps [`IsTerminal`] out of serif's
    /// public API.
    fn is_terminal(&self) -> bool {
        match self {
            Output::Stdout => std::io::stdout().is_terminal(),
            Output::Stderr => std::io::stderr().is_terminal(),
        }
    }
}

/// When to apply ANSI colors to output.
#[derive(Debug, Clone, Copy)]
pub enum ColorMode {
    /// Apply colors if the output (stdout or stderr) is a terminal. This is the default.
    ///
    /// Additionally, if the `NO_COLOR` environment variable is set to any non-empty string, ANSI
    /// coloring will be disabled.
    Auto,
    /// Always apply ANSI colors.
    Always,
    /// Never apply ANSI colors.
    Never,
}

impl Default for ColorMode {
    fn default() -> Self {
        Self::Auto
    }
}

impl ColorMode {
    /// Whether to enable ANSI colors for a given Output destination.
    fn enable_for(&self, output: Output) -> bool {
        match self {
            Self::Auto => {
                if env::var_os("NO_COLOR").map(|s| !s.is_empty()).unwrap_or(false) {
                    false
                } else {
                    output.is_terminal()
                }
            }
            Self::Always => true,
            Self::Never => false,
        }
    }
}

/// Builder style configuration for the `serif` tracing-subscriber implementation.
#[derive(Debug, Clone)]
pub struct Config {
    event_formatter: EventFormatter,
    output: Output,
    color: ColorMode,
    default_directive: Directive,
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

impl Config {
    // main builder methods

    /// Create a new `Config` with the default configuration.
    pub fn new() -> Self {
        Self {
            event_formatter: Default::default(),
            output: Default::default(),
            color: Default::default(),
            default_directive: LevelFilter::INFO.into(),
        }
    }

    /// Change the output destination to stdout or stderr. The default is stdout.
    pub fn with_output(self, output: Output) -> Self {
        Self { output, ..self }
    }

    /// Enable or disable ANSI coloring. The default is [`ColorMode::Auto`].
    pub fn with_color(self, color: ColorMode) -> Self {
        Self { color, ..self }
    }

    /// Set the default log directive. The default is the INFO level.
    ///
    /// You can call this with [`tracing::Level`] and [`tracing_subscriber::filter::LevelFilter`],
    /// since those types implement `Into<Directive>`.
    pub fn with_default(self, default: impl Into<Directive>) -> Self {
        Self { default_directive: default.into(), ..self }
    }

    /// Set the default log level using a numberic "verbosity" value.
    ///
    /// Applications can use this to easily turn the count of command line flags (e.g. `--verbose`
    /// or `--quiet`) into a default log level. This method does the same thing as
    /// [`Config::with_default`] and it makes no sense to combine them.
    ///
    /// The mapping of verbosity levels to log levels is:
    ///   * `-3` or less: off (no logs enabled)
    ///   * `-2`: error
    ///   * `-1`: warning
    ///   * `0`: info
    ///   * `1`: debug
    ///   * `2` or greater: trace
    pub fn with_verbosity(self, verbosity: i32) -> Self {
        let level = match verbosity.clamp(-3, 2) {
            -3 => LevelFilter::OFF,
            -2 => LevelFilter::ERROR,
            -1 => LevelFilter::WARN,
            0 => LevelFilter::INFO,
            1 => LevelFilter::DEBUG,
            2 => LevelFilter::TRACE,
            _ => unreachable!(),
        };
        self.with_default(level)
    }

    // EventFormatter builder methods

    /// Set the timestamp format for this Config.
    pub fn with_timestamp(self, time_format: TimeFormat) -> Self {
        Self { event_formatter: self.event_formatter.with_timestamp(time_format), ..self }
    }

    /// Set whether or not an event's target is displayed.
    pub fn with_target(self, display_target: bool) -> Self {
        Self { event_formatter: self.event_formatter.with_target(display_target), ..self }
    }

    /// Set whether or not an event's span scope is displayed.
    pub fn with_scope(self, display_scope: bool) -> Self {
        Self { event_formatter: self.event_formatter.with_scope(display_scope), ..self }
    }

    /// Finalize this Config and register it as the global default tracing subscriber.
    ///
    /// # Panics
    ///
    /// Panics if the `RUST_LOG` environment variable is invalid (see [`make_env_filter`]) or if
    /// another global subscriber is already installed (see [`SubscriberBuilder::init`]).
    ///
    /// [`make_env_filter`]: Config::make_env_filter
    /// [`SubscriberBuilder::init`]: tracing_subscriber::fmt::SubscriberBuilder::init
    pub fn init(self) {
        // FmtSubscriber (and SubscriberBuilder) are generic over the MakeWriter type given to
        // with_writer, so split up the logic to avoid having to wrap stdout/stderr in an extra
        // Box. Due to unnecessary implementation restrictions, with_ansi must be set before
        // setting the custom event formatter. See https://github.com/tokio-rs/tracing/issues/1867
        let builder = tracing_subscriber::fmt()
            .with_env_filter(self.make_env_filter())
            .with_ansi(self.color.enable_for(self.output))
            // register custom formatter types
            .event_format(self.event_formatter)
            .fmt_fields(FieldFormatter::new());

        match self.output {
            Output::Stdout => builder.with_writer(io::stdout).init(),
            Output::Stderr => builder.with_writer(io::stderr).init(),
        }
    }

    /// Create an [`EnvFilter`] from this Config.
    ///
    /// # Panics
    ///
    /// Panics if the `RUST_LOG` environment variable contains invalid unicode, or if it contains
    /// invalid [`EnvFilter`] directives.
    pub fn make_env_filter(&self) -> EnvFilter {
        // EnvFilter's handling of defaults and fallbacks is wonky and confusing (there's a number
        // of github issues so hopefully it's improved eventually). So we sidestep all that mess
        // and handle the logic ourselves. If RUST_LOG is unset or empty, then use our fallback. If
        // RUST_LOG is set, use it with no default/fallback, and use the try_new method to cause
        // errors on any invalid directives.
        let env_str = match env::var("RUST_LOG") {
            Ok(val) if val.is_empty() => None,
            Ok(val) => Some(val),
            Err(VarError::NotPresent) => None,
            Err(VarError::NotUnicode(val)) => {
                panic!("The RUST_LOG environment variable isn't valid unicode: {val:?}")
            }
        };

        match &env_str {
            Some(filter_str) => {
                debug_assert!(!filter_str.is_empty());
                EnvFilter::try_new(filter_str).unwrap_or_else(|err| {
                    panic!("Invalid RUST_LOG filter string '{filter_str}': {err}")
                })
            }
            None => EnvFilter::default().add_directive(self.default_directive.clone()),
        }
    }
}
