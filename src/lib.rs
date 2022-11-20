use std::fmt;

use chrono::{DateTime, Local, Utc};
use nu_ansi_term::{Color, Style};
use tracing_core::{field::Field, Event, Level, Subscriber};
use tracing_subscriber::{
    field::{MakeVisitor, Visit, VisitFmt, VisitOutput},
    fmt::{format::Writer, FmtContext, FormatEvent, FormatFields, FormattedFields},
    registry::LookupSpan,
};

#[cfg(feature = "re-exports")]
pub use tracing;

#[cfg(feature = "re-exports")]
pub mod macros {
    #[doc(no_inline)]
    pub use tracing::{debug, error, info, span, trace, warn};
}

mod config;
pub use config::{ColorMode, Config, Output};

/// Extension trait for writing ANSI-styled messages
trait WriterExt: fmt::Write {
    /// Whether or not ANSI formatting should be enabled.
    ///
    /// When this method returns `false`, calls to [`write_style`] will ignore the given style and
    /// write plain output instead.
    fn enable_ansi(&self) -> bool;

    /// Write any `Display`-able type to this Writer, using the given `Style` if and only if
    /// `enable_ansi` returns `true`
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

/// Macro to call [`WriterExt::write_style`] with arbitrary format arguments
macro_rules! write_style {
    ($writer:expr, $style:expr, $($arg:tt)*) => {
        $writer.write_style($style, format_args!($($arg)*))
    };
}

/// `serif`'s formatter for event and span metadata fields.
///
/// `FieldFormatter` is intended to be used with [`SubscriberBuilder::fmt_fields`]
///
/// [`SubscriberBuilder::fmt_fields`]: tracing_subscriber::fmt::SubscriberBuilder::fmt_fields
pub struct FieldFormatter {
    // reserve the right to add options in the future
    _private: (),
}

impl FieldFormatter {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for FieldFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> MakeVisitor<Writer<'a>> for FieldFormatter {
    type Visitor = FieldVisitor<'a>;

    fn make_visitor(&self, target: Writer<'a>) -> Self::Visitor {
        FieldVisitor::new(target)
    }
}

/// A type of field that's been visited. Implementation detail of [`FieldVisitor`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldType {
    None,
    Message,
    Other,
}

/// The visitor type used by [`FieldFormatter`]
///
/// If a field is named `"message"`, then it's printed in the default text style. All other fields
/// are formatted in square brackets and dimmed text style like `[name=value]`. Padding is added on
/// either side of the `"message"` field, but not around other fields.
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

    /// Implementation of `Visit::record_debug` but returning a Result for easier error handling.
    fn inner_record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) -> fmt::Result {
        let name = field.name();
        if name == "message" {
            let pad = match self.last {
                FieldType::None => "",
                FieldType::Message | FieldType::Other => " ",
            };
            write!(self.writer, "{pad}{value:?}")?;
            self.last = FieldType::Message;
        } else {
            let pad = match self.last {
                FieldType::Message => " ",
                FieldType::None | FieldType::Other => "",
            };
            write_style!(self.writer, Style::default().dimmed(), "{pad}[{name}={value:?}]")?;
            self.last = FieldType::Other;
        }

        Ok(())
    }
}

impl<'a> Visit for FieldVisitor<'a> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if self.result.is_err() {
            return;
        }
        self.result = self.inner_record_debug(field, value);
    }
}

impl<'a> VisitOutput<fmt::Result> for FieldVisitor<'a> {
    fn finish(self) -> fmt::Result {
        self.result
    }
}

impl<'a> VisitFmt for FieldVisitor<'a> {
    fn writer(&mut self) -> &mut dyn fmt::Write {
        &mut self.writer
    }
}

/// The style of timestamp to be formatted for tracing events
#[derive(Debug, Clone)]
pub enum TimeFormat {
    /// Don't display a timestamp
    None,
    /// Display a RFC 3339 timestamp in the local timezone. This is the default
    Local,
    /// Display a RFC 3339 timestamp in UTC
    Utc,
    /// Display a timestamp in the local timezone using a custom format.
    ///
    /// See [`chrono::format::strftime`] for the syntax
    CustomLocal(String),
    /// Display a timestamp in UTC using a custom format.
    ///
    /// See [`chrono::format::strftime`] for the syntax
    CustomUtc(String),
}

impl Default for TimeFormat {
    fn default() -> Self {
        Self::Local
    }
}

impl TimeFormat {
    fn is_none(&self) -> bool {
        matches!(self, TimeFormat::None)
    }
}

/// Helper to format a timestamp easily using Display
struct TimeDisplay<'a>(&'a TimeFormat, DateTime<Utc>);

impl fmt::Display for TimeDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.0 {
            TimeFormat::None => Ok(()),
            TimeFormat::Local => DateTime::<Local>::from(self.1).format("%FT%T%z").fmt(f),
            TimeFormat::Utc => self.1.format("%FT%TZ").fmt(f),
            TimeFormat::CustomLocal(fmt) => DateTime::<Local>::from(self.1).format(fmt).fmt(f),
            TimeFormat::CustomUtc(fmt) => self.1.format(fmt).fmt(f),
        }
    }
}

/// `serif`'s main tracing event formatter.
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

    /// Set the timestamp format for this event formatter
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
        let dimmed = Style::default().dimmed();

        // display the timestamp
        if !self.time_format.is_none() {
            write_style!(writer, dimmed, "[{}] ", TimeDisplay(&self.time_format, Utc::now()))?;
        }

        // display the level
        let level = *event.metadata().level();
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
            write_style!(writer, Color::Blue.dimmed(), "{}", event.metadata().target())?;
            writer.write_str(": ")?;
        }

        // display the event message and fields
        ctx.format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}
