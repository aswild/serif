mod yak_shave;

fn main() {
    tracing_init(Some(2));

    let number_of_yaks = 3;
    // this creates a new event, outside of any spans.
    tracing::info!(number_of_yaks, "preparing to shave yaks");

    let number_shaved = yak_shave::shave_all(number_of_yaks);
    tracing::info!(all_yaks_shaved = number_shaved == number_of_yaks, "yak shaving completed.");
}

use std::fmt;

use nu_ansi_term::{Color, Style};
use tracing::{field::Field, Event, Level, Subscriber};
use tracing_subscriber::{
    field::{MakeVisitor, Visit, VisitFmt, VisitOutput},
    fmt::{format::Writer, FmtContext, FormatEvent, FormatFields, FormattedFields},
    registry::LookupSpan,
    EnvFilter,
};

trait WriterExt: fmt::Write {
    fn style(&self, style: impl Into<Style>) -> Style;

    fn write_style<S, T>(&mut self, style: S, value: T) -> fmt::Result
    where
        S: Into<Style>,
        T: fmt::Display,
    {
        let style = self.style(style);
        write!(self, "{}{}{}", style.prefix(), value, style.suffix())
    }
}

impl WriterExt for Writer<'_> {
    fn style(&self, style: impl Into<Style>) -> Style {
        if self.has_ansi_escapes() {
            style.into()
        } else {
            Style::default()
        }
    }
}

macro_rules! write_style {
    ($writer:expr, $style:expr, $($arg:tt)*) => {
        $writer.write_style($style, format_args!($($arg)*))
    };
}

struct FieldFormatter;

impl<'a> MakeVisitor<Writer<'a>> for FieldFormatter {
    type Visitor = FieldVisitor<'a>;

    fn make_visitor(&self, target: Writer<'a>) -> Self::Visitor {
        FieldVisitor::new(target)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldType {
    None,
    Message,
    Other,
}

struct FieldVisitor<'a> {
    writer: Writer<'a>,
    result: fmt::Result,
    last: FieldType,
}

impl<'a> FieldVisitor<'a> {
    fn new(writer: Writer<'a>) -> Self {
        Self { writer, result: Ok(()), last: FieldType::None }
    }

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

struct EventFormatter;

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
        write_style!(writer, dimmed, "[{}] ", chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%z"))?;

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
        if let Some(scope) = ctx.event_scope() {
            let mut seen = false;

            for span in scope.from_root() {
                //write_style!(writer, Color::Cyan.dimmed(), "{}", span.metadata().name())?;
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

        // display the target
        write_style!(writer, Color::Blue.dimmed(), "{}", event.metadata().target())?;
        writer.write_str(": ")?;

        // display the event message and fields
        ctx.format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

fn tracing_init(verbosity: Option<i32>) {
    let default_filter_str = match verbosity.map(|val| val.clamp(-3, 2)) {
        Some(-3) => "off",
        Some(-2) => "error",
        Some(-1) => "warn",
        Some(0) | None => "info",
        Some(1) => "debug",
        Some(2) => "trace",
        Some(_) => unreachable!(),
    };

    let env = std::env::var("RUST_LOG"); // owns a String, keep it live on the stack
    let filter_string = match env {
        Ok(ref val) => {
            if val.is_empty() {
                default_filter_str
            } else {
                val
            }
        }
        Err(std::env::VarError::NotPresent) => default_filter_str,
        Err(std::env::VarError::NotUnicode(_)) => {
            panic!("RUST_LOG environment variable isn't valid unicode")
        }
    };

    debug_assert!(!filter_string.is_empty());
    let filter = match EnvFilter::try_new(filter_string) {
        Ok(filter) => filter,
        Err(err) => panic!("Invalid RUST_LOG filter string '{filter_string}': {err}"),
    };

    tracing_subscriber::fmt()
        // use our event filter from above
        .with_env_filter(filter)
        // due to unnecessary implementation restrictions, with_ansi must be set before registering
        // the custom event formatter. See https://github.com/tokio-rs/tracing/issues/1867
        .with_ansi(true)
        // register custom formatter types
        .event_format(EventFormatter)
        .fmt_fields(FieldFormatter)
        // write to stderr instead of stdout
        .with_writer(std::io::stderr)
        // register as the global default subscriber
        .init();
}

/*
/// tracing-subscriber uses the `time` crate, which doesn't have a safe way to get the local time.
/// `chrono`, however, has implemented safe local time support, so use that.
struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut Writer) -> fmt::Result {
        //let now = chrono::Local::now();
        //write!(w, "[{}]", now.format("%Y-%m-%d %H:%M:%S%z"))
        write!(w, "[{}]", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%z"))
    }
}
*/
