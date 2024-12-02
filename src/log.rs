use anyhow::Result;
use std::fmt;
use time::UtcOffset;
use tracing::Subscriber;
use tracing_subscriber::{
    fmt::{format::Writer, FmtContext, FormatEvent, FormatFields},
    registry::LookupSpan,
    EnvFilter,
};

/// Custom event formatter that mimics Google Cloud (absl) logging format
pub struct GoogleEventFormat;

impl<S, N> FormatEvent<S, N> for GoogleEventFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> fmt::Result {
        // Get local time with fallback to UTC
        let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| {
            time::OffsetDateTime::now_utc().to_offset(UtcOffset::from_hms(0, 0, 0).unwrap())
        });

        // Format timestamp in Google style: MMDD HH:MM:SS.mmm
        write!(
            writer,
            "{:02}{:02} {:02}:{:02}:{:02}.{:03} ",
            now.month() as u8,
            now.day(),
            now.hour(),
            now.minute(),
            now.second(),
            now.millisecond()
        )?;

        // Format level in a consistent width
        let level = event.metadata().level();
        write!(writer, "{:5} ", level.as_str())?;

        // Add module path without brackets
        if let Some(module_path) = event.metadata().module_path() {
            let root_module = module_path.split("::").next().unwrap_or(module_path);
            write!(writer, "{} ", root_module)?;
        }

        // Add file and line information
        if let Some(file) = event.metadata().file() {
            write!(
                writer,
                "{}:{}] ",
                file.split('/').last().unwrap_or(file),
                event.metadata().line().unwrap_or(0)
            )?;
        }

        // Format the actual event data
        ctx.field_format().format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}
