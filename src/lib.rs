// Copyright 2022-2025 Allen Wild
// SPDX-License-Identifier: Apache-2.0
//! Serif is an opinionated Rust tracing-subscriber configuration with a focus on readability.
//! ## About
//!
//! Serif is my take on the best way to configure [`tracing-subscriber`] for use in command-line
//! applications, with an emphasis on readability of the main log messages. The tracing span scope,
//! event target, and additional metadata is all rendered with dimmed colors, making the main
//! message stand out quickly. Or at least it does on the Solarized Dark colorscheme that I prefer.
//!
//! Serif uses [`EnvFilter`] for filtering using the `RUST_LOG` environment variable, with a default
//! level of `INFO` if not otherwise configured.
//!
//! Serif sets up [`FmtSubscriber`] and [`EnvFilter`] in a unified configuration. Basically this is
//! all to make my life easier migrating from [`env_logger`].
//!
//! ## Usage
//!
//! All you need is a single dependency in `Cargo.toml` and a single builder chain to set up the
//! global default tracing subscriber.  For convenience, `serif` re-exports `tracing` and provides
//! the common log macros in `serif::macros`.
//!
//! ```
//! use serif::macros::*;
//! use serif::tracing::Level;
//!
//! # fn do_stuff() {}
//! fn main() {
//!     serif::Config::new()            // create config builder
//!         .with_default(Level::DEBUG) // the default otherwise is INFO
//!         .init();                    // finalize and register with tracing
//!     info!("Hello World!");
//!     do_stuff();
//!     debug!("Finished doing stuff");
//! }
//! ```
//!
//! For more advanced use-cases, Serif provides [`EventFormatter`] which implements
//! [`FormatEvent`], and [`FieldFormatter`] which implements [`FormatFields`]. These objects can be
//! passed to a [`SubscriberBuilder`] along with whatever other options are desired.
//!
//! ## ANSI Terminal Colors
//!
//! By default, Serif enables ANSI coloring when the output file descriptor (stdout or stderr) is
//! a TTY and the environment variable `NO_COLOR` is either unset or empty. At the moment, the
//! specific color styles are not customizable.
//!
//! A note to advanced users configuring a [`SubscriberBuilder`] manually: `EventFormatter` and
//! `FieldFormatter` do not track whether ANSI colors are enabled directly, instead they obtain
//! this from the [`Writer`] that's passed to various methods. Call
//! [`SubscriberBuilder::with_ansi`] to configure coloring in custom usage.
//!
//! [`tracing-subscriber`]: https://lib.rs/crates/tracing-subscriber
//! [`FmtSubscriber`]: tracing_subscriber::fmt::Subscriber
//! [`EnvFilter`]: tracing_subscriber::EnvFilter
//! [`env_logger`]: https://lib.rs/crates/env_logger
//! [`SubscriberBuilder`]: tracing_subscriber::fmt::SubscriberBuilder
//! [`SubscriberBuilder::with_ansi`]: tracing_subscriber::fmt::SubscriberBuilder::with_ansi

#![warn(missing_docs)]
#![warn(clippy::all)]

use std::fmt;

use jiff::{tz::TimeZone, Timestamp, Zoned};
use nu_ansi_term::{Color, Style};
use tracing_core::{field::Field, Event, Level, Subscriber};
use tracing_log::NormalizeEvent;
use tracing_subscriber::{
    field::{MakeVisitor, Visit, VisitFmt, VisitOutput},
    fmt::{format::Writer, FmtContext, FormatEvent, FormatFields, FormattedFields},
    registry::LookupSpan,
};

#[cfg(feature = "re-exports")]
pub use tracing;

/// Convenient re-exports of macros from the [`tracing`] crate. This module is intended to be
/// glob-imported like a prelude.
#[cfg(feature = "re-exports")]
pub mod macros {
    #[doc(no_inline)]
    pub use tracing::{
        debug, debug_span, enabled, error, error_span, event, event_enabled, info, info_span, span,
        span_enabled, trace, trace_span, warn, warn_span,
    };
}

mod config;
pub use config::{ColorMode, Config, Output};

/// Extension trait for writing ANSI-styled messages.
trait WriterExt: fmt::Write {
    /// Whether or not ANSI formatting should be enabled.
    ///
    /// When this method returns `false`, calls to [`write_style`] will ignore the given style and
    /// write plain output instead.
    fn enable_ansi(&self) -> bool;

    /// Write any `Display`-able type to this Writer, using the given `Style` if and only if
    /// `enable_ansi` returns `true`.
    fn write_style<S, T>(&mut self, style: S, value: T) -> fmt::Result
    where
        S: Into<Style>,
        T: fmt::Display,
    {
        if self.enable_ansi() {
            let style = style.into();
            write!(self, "{}{}{}", style.prefix(), value, style.suffix())
        } else {
            write!(self, "{}", value)
        }
    }
}

impl WriterExt for Writer<'_> {
    #[inline]
    fn enable_ansi(&self) -> bool {
        self.has_ansi_escapes()
    }
}

/// Macro to call [`WriterExt::write_style`] with arbitrary format arguments.
macro_rules! write_style {
    ($writer:expr, $style:expr, $($arg:tt)*) => {
        $writer.write_style($style, format_args!($($arg)*))
    };
}

/// Serif's formatter for event and span metadata fields.
///
/// `FieldFormatter` is intended to be used with [`SubscriberBuilder::fmt_fields`] and is designed
/// to work with [`EventFormatter`]'s output format.
///
/// `FieldFormatter` implements [`FormatFields`], though this isn't immediately clear in Rustdoc.
/// Specifically, `FieldFormatter` implements [`MakeVisitor`], and [`FieldVisitor`] implements
/// [`Visit`], [`VisitOutput`], and [`VisitFmt`]. Thanks to blanket impls in the
/// [`tracing_subscriber`] crate, this means that `FieldFormatter` implements [`FormatFields`].
///
/// # Field Format
/// If a field is named `message`, then it's printed in the default text style. All other fields
/// are formatted in square brackets and dimmed text style like `[name=value]`. Padding is added on
/// either side of the `message` field, but not around other fields.
///
/// [`SubscriberBuilder::fmt_fields`]: tracing_subscriber::fmt::SubscriberBuilder::fmt_fields
#[derive(Clone)]
pub struct FieldFormatter {
    // reserve the right to add options in the future
    _private: (),
}

impl FieldFormatter {
    /// Create a new `FieldFormatter` with the default configuration.
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for FieldFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for FieldFormatter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad("FieldFormatter")
    }
}

impl<'a> MakeVisitor<Writer<'a>> for FieldFormatter {
    type Visitor = FieldVisitor<'a>;

    fn make_visitor(&self, target: Writer<'a>) -> Self::Visitor {
        FieldVisitor::new(target)
    }
}

/// A type of field that's been visited. Implementation detail of [`FieldVisitor`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldType {
    None,
    Message,
    Other,
}

/// The visitor type used by [`FieldFormatter`]
///
/// If a field is named `message`, then it's printed in the default text style. All other fields
/// are formatted in square brackets and dimmed text style like `[name=value]`. Padding is added on
/// either side of the `message` field, but not around other fields. [`Error`] typed fields are
/// rendered in dimmed red text.
///
/// [`Error`]: std::error::Error
#[derive(Debug)]
pub struct FieldVisitor<'a> {
    writer: Writer<'a>,
    result: fmt::Result,
    last: FieldType,
}

impl<'a> FieldVisitor<'a> {
    /// Create a new `FieldVisitor` with the given writer.
    pub fn new(writer: Writer<'a>) -> Self {
        Self { writer, result: Ok(()), last: FieldType::None }
    }

    /// Get the padding that should be prepended when visiting the message field
    fn pad_for_message(&self) -> &'static str {
        match self.last {
            FieldType::None => "",
            FieldType::Message | FieldType::Other => " ",
        }
    }

    /// Get the padding that should be prepended when visiting a non-message field
    fn pad_for_other(&self) -> &'static str {
        match self.last {
            FieldType::Message => " ",
            FieldType::None | FieldType::Other => "",
        }
    }
}

impl Visit for FieldVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if self.result.is_err() {
            return;
        }

        let name = field.name();
        if name.starts_with("log.") {
            // skip log metadata
            return;
        }

        self.result = if name == "message" {
            let pad = self.pad_for_message();
            self.last = FieldType::Message;
            write!(self.writer, "{pad}{value:?}")
        } else {
            let pad = self.pad_for_other();
            self.last = FieldType::Other;
            write_style!(self.writer, Style::default().dimmed(), "{pad}[{name}={value:?}]")
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            // Usually the message gets visited by record_debug, presumably becuase it's
            // a fmt::Arguments object from a format_args! macro, but just in case the message
            // field ends up here, force using the Display impl to render without quotes.
            self.record_debug(field, &format_args!("{value}"));
        } else {
            // Otherwise, delegate to record_debug as usual.
            self.record_debug(field, &value);
        }
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        if self.result.is_err() {
            return;
        }

        let name = field.name();
        if name.starts_with("log.") {
            // skip log metadata
            return;
        }

        // Treat Errors like a non-message field, and make them red.
        let pad = self.pad_for_other();
        self.last = FieldType::Other;
        self.result = write_style!(self.writer, Color::Red.dimmed(), "{pad}[{name}={value}]");
    }
}

impl VisitOutput<fmt::Result> for FieldVisitor<'_> {
    fn finish(self) -> fmt::Result {
        self.result
    }
}

impl VisitFmt for FieldVisitor<'_> {
    fn writer(&mut self) -> &mut dyn fmt::Write {
        &mut self.writer
    }
}

/// The style of timestamp to be formatted for tracing events.
///
/// Format strings are used by [`chrono::format::strftime`], and local timezone handling is
/// provided by the [`chrono`] crate.
#[derive(Clone)]
pub struct TimeFormat {
    inner: InnerTimeFormat,
}

/// Private implementation for TimeFormat
#[derive(Clone)]
enum InnerTimeFormat {
    None,
    Local(Option<Box<str>>),
    Utc(Option<Box<str>>),
}

impl Default for TimeFormat {
    fn default() -> Self {
        Self::local()
    }
}

impl fmt::Debug for TimeFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.inner {
            InnerTimeFormat::None => f.write_str("TimeFormat::None"),
            InnerTimeFormat::Local(format) => write!(f, "TimeFormat::Local({format:?})"),
            InnerTimeFormat::Utc(format) => write!(f, "TimeFormat::Utc({format:?})"),
        }
    }
}

impl TimeFormat {
    /// RFC 3339 timestamp enclosed in square brackets, with offset.
    pub const LOCAL_FORMAT: &'static str = "[%Y-%m-%dT%H:%M:%S%z]";

    /// RFC 3339 timestamp enclosed in square brackets, with UTC (using 'Z' for the timezone
    /// instead of '+0000')
    pub const UTC_FORMAT: &'static str = "[%Y-%m-%dT%H:%M:%SZ]";

    /// Do not render a timestamp.
    pub const fn none() -> Self {
        Self { inner: InnerTimeFormat::None }
    }

    /// Render a timestamp in the local timezone using the default format.
    pub const fn local() -> Self {
        Self { inner: InnerTimeFormat::Local(None) }
    }

    /// Render a timestamp in UTC using the default format.
    pub const fn utc() -> Self {
        Self { inner: InnerTimeFormat::Utc(None) }
    }

    /// Render a timestamp in the local timezone using a custom format.
    ///
    /// **Panics:** When `debug_assertions` are enabled, the format string is validated to ensure
    /// that no unknown `%` fields are present. In release mode, formatting the timestamp fails and
    /// tracing-subscriber will emit "Unable to format the following event" messages.
    pub fn local_custom(format: impl Into<String>) -> Self {
        let format = format.into();

        #[cfg(debug_assertions)]
        {
            let zoned = Zoned::new(Timestamp::UNIX_EPOCH, TimeZone::UTC);
            let res = jiff::fmt::strtime::format(format.as_bytes(), &zoned);
            if let Err(err) = res {
                panic!("Unable to use custom TimeFormat '{format}': {err}");
            }
        }

        Self { inner: InnerTimeFormat::Local(Some(format.into_boxed_str())) }
    }

    /// Render a timestamp in UTC using a custom format.
    ///
    /// **Panics:** When `debug_assertions` are enabled, the format string is validated to ensure
    /// that no unknown `%` fields are present. In release mode, formatting the timestamp fails and
    /// tracing-subscriber will emit "Unable to format the following event" messages.
    pub fn utc_custom(format: impl Into<String>) -> Self {
        let format = format.into();

        #[cfg(debug_assertions)]
        {
            let res = jiff::fmt::strtime::format(format.as_bytes(), Timestamp::UNIX_EPOCH);
            if let Err(err) = res {
                panic!("Unable to use custom TimeFormat '{format}': {err}");
            }
        }

        Self { inner: InnerTimeFormat::Utc(Some(format.into_boxed_str())) }
    }

    /// Get a [`Display`]-able object of this format applied to a `Timestamp`.
    pub fn render(&self, ts: Timestamp) -> impl fmt::Display + '_ {
        TimeDisplay(self, ts)
    }

    /// Render the current system time in this format
    pub fn render_now(&self) -> impl fmt::Display + '_ {
        self.render(Timestamp::now())
    }

    fn is_none(&self) -> bool {
        matches!(self.inner, InnerTimeFormat::None)
    }
}

/// Helper to format a timestamp easily using Display
struct TimeDisplay<'a>(&'a TimeFormat, Timestamp);

impl fmt::Display for TimeDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.0.inner {
            InnerTimeFormat::None => Ok(()),
            InnerTimeFormat::Local(format) => {
                let format = format.as_deref().unwrap_or(TimeFormat::LOCAL_FORMAT);
                let zoned = Zoned::new(self.1, TimeZone::system());
                let disp = zoned.strftime(format.as_bytes());
                fmt::Display::fmt(&disp, f)
            }
            InnerTimeFormat::Utc(format) => {
                let format = format.as_deref().unwrap_or(TimeFormat::UTC_FORMAT);
                let disp = self.1.strftime(format.as_bytes());
                fmt::Display::fmt(&disp, f)
            }
        }
    }
}

/// Serif's tracing event formatter.
///
/// # Event Format
/// Events are rendered similarly to [`tracing_subscriber::fmt::format::Full`], but with everything
/// besides the main log message in dimmed ANSI text colors to increase readability of the main log
/// message.
#[derive(Debug, Clone)]
pub struct EventFormatter {
    time_format: TimeFormat,
    display_target: bool,
    display_scope: bool,
}

impl EventFormatter {
    /// Create a new `EventFormatter` with the default options.
    pub fn new() -> Self {
        Self { time_format: Default::default(), display_target: true, display_scope: true }
    }

    /// Set the timestamp format for this event formatter.
    pub fn with_timestamp(self, time_format: TimeFormat) -> Self {
        Self { time_format, ..self }
    }

    /// Set whether or not an event's target is displayed.
    pub fn with_target(self, display_target: bool) -> Self {
        Self { display_target, ..self }
    }

    /// Set whether or not an event's span scope is displayed.
    pub fn with_scope(self, display_scope: bool) -> Self {
        Self { display_scope, ..self }
    }
}

impl Default for EventFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl<S, N> FormatEvent<S, N> for EventFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        // normalize event metadata in case this even was a log message
        let norm_meta = event.normalized_metadata();
        let meta = norm_meta.as_ref().unwrap_or_else(|| event.metadata());

        // display the timestamp
        if !self.time_format.is_none() {
            write_style!(writer, Style::default().dimmed(), "{} ", self.time_format.render_now(),)?;
        }

        // display the level
        let level = *meta.level();
        let level_style = match level {
            Level::TRACE => Color::Purple,
            Level::DEBUG => Color::Blue,
            Level::INFO => Color::Green,
            Level::WARN => Color::Yellow,
            Level::ERROR => Color::Red,
        };
        write_style!(writer, level_style, "{level:>5} ")?;

        // display the span's scope
        let maybe_scope = if self.display_scope { ctx.event_scope() } else { None };
        if let Some(scope) = maybe_scope {
            let mut seen = false;

            for span in scope.from_root() {
                writer.write_style(Color::Cyan.dimmed(), span.metadata().name())?;
                seen = true;

                if let Some(fields) = span.extensions().get::<FormattedFields<N>>() {
                    if !fields.is_empty() {
                        write!(writer, "{}:", fields)?;
                    }
                }
            }

            if seen {
                writer.write_char(' ')?;
            }
        }

        // display the target (which is the rust module path by default, but can be overridden)
        if self.display_target {
            write_style!(writer, Color::Blue.dimmed(), "{}", meta.target())?;
            writer.write_str(": ")?;
        }

        // display the event message and fields
        ctx.format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}
