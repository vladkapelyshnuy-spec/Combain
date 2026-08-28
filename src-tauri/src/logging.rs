use tracing_subscriber::EnvFilter;

/// Initialize process-wide tracing. Safe to call once at startup.
pub fn init(log_level: &str) {
    let filter = EnvFilter::try_new(log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .try_init();
}
