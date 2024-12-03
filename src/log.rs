use std::fmt;
use time::UtcOffset;
use tracing::field::{Field, Visit};
use tracing::Subscriber;
use tracing_subscriber::{
    field::RecordFields,
    fmt::{format::Writer, FmtContext, FormatEvent, FormatFields},
    registry::LookupSpan,
};

/// Custom field formatter that disables ANSI colors
pub struct GoogleFormatFields;

impl<'writer> FormatFields<'writer> for GoogleFormatFields {
    fn format_fields<R: RecordFields>(
        &self,
        mut writer: Writer<'writer>,
        fields: R,
    ) -> fmt::Result {
        let mut visitor = FieldVisitor {
            writer: &mut writer,
            is_first: true,
        };
        fields.record(&mut visitor);
        Ok(())
    }
}

struct FieldVisitor<'a, 'b> {
    writer: &'a mut Writer<'b>,
    is_first: bool,
}

impl<'a, 'b> Visit for FieldVisitor<'a, 'b> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if !self.is_first {
            let _ = write!(self.writer, " ");
        }
        if field.name() != "message" {
            let _ = write!(self.writer, "{}=", field.name());
        }
        let _ = write!(self.writer, "{:?}", value);
        self.is_first = false;
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if !self.is_first {
            let _ = write!(self.writer, " ");
        }
        if field.name() != "message" {
            let _ = write!(self.writer, "{}=\"{}\"", field.name(), value);
        } else {
            let _ = write!(self.writer, "{}", value);
        }
        self.is_first = false;
    }
}

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
        // Format level in a consistent width
        let level = event.metadata().level();
        write!(writer, "{}", level.as_str().chars().next().unwrap())?;

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

        // Format the actual event data using GoogleFormatFields
        ctx.format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}
