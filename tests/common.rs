use git_remote_s3::log::{GoogleEventFormat, GoogleFormatFields};
use std::sync::Once;
use tracing::debug;
use tracing_subscriber::EnvFilter;

pub static INIT_LOGGER: Once = Once::new();

/// Initialize logging for tests with consistent configuration
pub fn init_test_logging() {
    INIT_LOGGER.call_once(|| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("error,git_remote_s3=debug,main_test=debug,tests=debug"));

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .event_format(GoogleEventFormat)
            .fmt_fields(GoogleFormatFields)
            .with_test_writer()
            .init();

        debug!("Test logging initialized");
    });
}
