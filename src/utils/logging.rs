//! Logging utilities for the application
//!
//! This module provides utilities for setting up and configuring logging for the application.
//! It uses the `tracing_subscriber` crate to configure the logging.
//!
//! The `setup_logging` function sets up the logging for the application.
//! It uses the `tracing_subscriber` crate to configure the logging.
use tracing_subscriber::{filter::EnvFilter, fmt, prelude::*};

/// Setup logging for the application
///
/// This function sets up the logging for the application.
/// It uses the `tracing_subscriber` crate to configure the logging.
pub fn setup_logging() {
	// Create a filter based on environment variable or default to INFO
	let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

	// Create a subscriber that uses the filter and a console output
	let subscriber = tracing_subscriber::registry().with(filter).with(
		fmt::layer()
			.with_writer(std::io::stdout)
			.event_format(
				fmt::format()
					.with_level(true)
					.with_target(true)
					.with_thread_ids(false)
					.with_thread_names(false)
					.with_ansi(true)
					.compact(),
			)
			.fmt_fields(fmt::format::PrettyFields::new()),
	);

	// Try to set the subscriber, but don't panic if it fails
	let _ = subscriber.try_init();
}
