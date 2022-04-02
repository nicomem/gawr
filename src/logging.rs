use miette::{Context, IntoDiagnostic, Result};
use owo_colors::OwoColorize;
use time::{
    format_description::{self, FormatItem},
    OffsetDateTime, UtcOffset,
};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::{
    fmt::{format, FmtContext, FormatEvent, FormatFields},
    registry::LookupSpan,
    FmtSubscriber,
};

/// Initialize the logging system
pub fn init_logging(level: tracing::Level) -> Result<()> {
    let local_offset = UtcOffset::current_local_offset()
        .into_diagnostic()
        .wrap_err("Could not get current local time offet")?;

    let my_pretty_logger = MyPrettyLogger::new(local_offset);

    let subscriber = FmtSubscriber::builder()
        .event_format(my_pretty_logger)
        .with_max_level(level)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .into_diagnostic()
        .wrap_err("Setting default subscriber failed")
}

/// Custom logger as the default ones are not as customizable as I want
struct MyPrettyLogger {
    offset: UtcOffset,
    time_format: Vec<FormatItem<'static>>,
}

impl MyPrettyLogger {
    fn new(offset: UtcOffset) -> Self {
        Self {
            offset,
            time_format: format_description::parse("[hour]:[minute]:[second]").unwrap(),
        }
    }
}

impl<S, N> FormatEvent<S, N> for MyPrettyLogger
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: format::Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        let metadata = event.metadata();

        let now = OffsetDateTime::now_utc().to_offset(self.offset).time();
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap();

        if writer.has_ansi_escapes() {
            let level = match *metadata.level() {
                Level::ERROR => metadata.level().red().to_string(),
                Level::WARN => metadata.level().yellow().to_string(),
                Level::DEBUG => metadata.level().blue().to_string(),
                _ => metadata.level().green().to_string(),
            };

            write!(
                &mut writer,
                "{} {:>5} {} ",
                now.format(&self.time_format).unwrap(),
                level,
                thread_name.yellow(),
            )?;
        } else {
            write!(
                &mut writer,
                "{} {:>5} {} ",
                now.format(&self.time_format).unwrap(),
                metadata.level(),
                thread_name,
            )?;
        }

        ctx.field_format().format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}
