/// Initialize tracing with environment-based filtering
///
/// Sets up a formatted tracing subscriber that:
/// - Reads log level from RUST_LOG environment variable
/// - Defaults to "info" level if RUST_LOG is not set
/// - Outputs logs with timestamps, levels, and module paths
///
/// # Example
/// ```no_run
/// casper_common::init_tracing();
/// tracing::info!("Application started");
/// ```
pub fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let use_json = std::env::var("LOG_FORMAT")
        .map(|v| v.eq_ignore_ascii_case("json"))
        .unwrap_or(false);
    if use_json {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}
