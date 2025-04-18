//! ## Sets up logging by reading configuration from environment variables.
//!
//! Environment variables used:
//! - LOG_MODE: "stdout" (default) or "file"
//! - LOG_LEVEL: log level ("trace", "debug", "info", "warn", "error"); default is "info"
//! - LOG_DATA_DIR: directory for log files; default is "logs/"
//! - LOG_MAX_SIZE: maximum size of log files in bytes; default is 1GB
//! - IN_DOCKER: "true" if running in Docker; default is "false"

pub mod error;

use chrono::Utc;
use std::{
	env,
	fs::{create_dir_all, metadata},
	path::Path,
};
use tracing::info;
use tracing_appender;
use tracing_subscriber::{filter::EnvFilter, fmt, prelude::*};

use tracing::Subscriber;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

/// Custom formatter that strips ANSI escape codes from log output
struct StripAnsiFormatter<T> {
	inner: T,
}

impl<T> StripAnsiFormatter<T> {
	fn new(inner: T) -> Self {
		Self { inner }
	}
}

impl<S, N, T> FormatEvent<S, N> for StripAnsiFormatter<T>
where
	S: Subscriber + for<'a> LookupSpan<'a>,
	N: for<'a> FormatFields<'a> + 'static,
	T: FormatEvent<S, N>,
{
	fn format_event(
		&self,
		ctx: &FmtContext<'_, S, N>,
		mut writer: Writer<'_>,
		event: &tracing::Event<'_>,
	) -> std::fmt::Result {
		// Create a buffer to capture the formatted output
		let mut buf = String::new();
		let string_writer = Writer::new(&mut buf);

		// Format the event using the inner formatter
		self.inner.format_event(ctx, string_writer, event)?;

		// Strip ANSI escape codes
		let stripped = strip_ansi_escapes(&buf);

		// Write the stripped string to the output
		write!(writer, "{}", stripped)
	}
}

/// Strips ANSI escape codes from a string
fn strip_ansi_escapes(s: &str) -> String {
	// Simple regex to match ANSI escape sequences
	// This matches the most common escape sequences like color codes
	let re = regex::Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap();
	re.replace_all(s, "").to_string()
}

/// Computes the path of the rolled log file given the base file path and the date string.
pub fn compute_rolled_file_path(base_file_path: &str, date_str: &str, index: u32) -> String {
	let trimmed = base_file_path
		.strip_suffix(".log")
		.unwrap_or(base_file_path);
	format!("{}-{}.{}.log", trimmed, date_str, index)
}

/// Checks if the given log file exceeds the maximum allowed size (in bytes).
/// If so, it appends a sequence number to generate a new file name.
/// Returns the final log file path to use.
/// - `file_path`: the initial time-based log file path.
/// - `base_file_path`: the original base log file path.
/// - `date_str`: the current date string.
/// - `max_size`: maximum file size in bytes (e.g., 1GB).
pub fn space_based_rolling(
	file_path: &str,
	base_file_path: &str,
	date_str: &str,
	max_size: u64,
) -> String {
	let mut final_path = file_path.to_string();
	let mut index = 1;
	while let Ok(metadata) = metadata(&final_path) {
		if metadata.len() > max_size {
			final_path = compute_rolled_file_path(base_file_path, date_str, index);
			index += 1;
		} else {
			break;
		}
	}
	final_path
}

/// Creates a log format with configurable ANSI support
fn create_log_format(with_ansi: bool) -> fmt::format::Format<fmt::format::Compact> {
	fmt::format()
		.with_level(true)
		.with_target(true)
		.with_thread_ids(false)
		.with_thread_names(false)
		.with_ansi(with_ansi)
		.compact()
}

/// Sets up logging by reading configuration from environment variables.
pub fn setup_logging() -> Result<(), Box<dyn std::error::Error>> {
	let log_mode = env::var("LOG_MODE").unwrap_or_else(|_| "stdout".to_string());
	let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());

	// Parse the log level
	let level_filter = match log_level.to_lowercase().as_str() {
		"trace" => tracing::Level::TRACE,
		"debug" => tracing::Level::DEBUG,
		"info" => tracing::Level::INFO,
		"warn" => tracing::Level::WARN,
		"error" => tracing::Level::ERROR,
		_ => tracing::Level::INFO,
	};

	// Create a format with ANSI disabled for file logging and enabled for stdout
	let with_ansi = log_mode.to_lowercase() != "file";
	let format = create_log_format(with_ansi);

	// Create a subscriber with the specified log level
	let subscriber = tracing_subscriber::registry().with(EnvFilter::new(level_filter.to_string()));

	if log_mode.to_lowercase() == "file" {
		info!("Logging to file: {}", log_level);

		// Use logs/ directly in container path, otherwise use LOG_DATA_DIR or default to logs/ for host path
		let log_dir = env::var("IN_DOCKER")
			.map(|val| val == "true")
			.unwrap_or(false)
			.then(|| "logs/".to_string())
			.unwrap_or_else(|| env::var("LOG_DATA_DIR").unwrap_or_else(|_| "logs/".to_string()));

		let log_dir = format!("{}/", log_dir.trim_end_matches('/'));
		// set dates
		let now = Utc::now();
		let date_str = now.format("%Y-%m-%d").to_string();

		// Get log file path from environment or use default
		let base_file_path = format!("{}monitor.log", log_dir);

		// verify the log file already exists
		if Path::new(&base_file_path).exists() {
			info!(
				"Base Log file already exists: {}. Proceeding to compute rolled log file path.",
				base_file_path
			);
		}

		// Time-based rolling: compute file name based on the current UTC date.
		let time_based_path = compute_rolled_file_path(&base_file_path, &date_str, 1);

		// Ensure parent directory exists.
		if let Some(parent) = Path::new(&time_based_path).parent() {
			create_dir_all(parent).expect("Failed to create log directory");
		}

		// Space-based rolling: if an existing log file exceeds 1GB, adopt a new file name.
		let max_size = parse_log_max_size();

		let final_path =
			space_based_rolling(&time_based_path, &base_file_path, &date_str, max_size);

		// Create a file appender
		let file_appender = tracing_appender::rolling::never(
			Path::new(&final_path).parent().unwrap_or(Path::new(".")),
			Path::new(&final_path).file_name().unwrap_or_default(),
		);

		let ansi_stripped_format = StripAnsiFormatter::new(format);

		subscriber
			.with(
				fmt::layer()
					.event_format(ansi_stripped_format)
					.with_writer(file_appender)
					.fmt_fields(fmt::format::PrettyFields::new()),
			)
			.init();
	} else {
		// Initialize the subscriber with stdout
		subscriber
			.with(
				fmt::layer()
					.event_format(format)
					.fmt_fields(fmt::format::PrettyFields::new()),
			)
			.init();
	}

	info!("Logging is successfully configured (mode: {})", log_mode);
	Ok(())
}

fn parse_log_max_size() -> u64 {
	env::var("LOG_MAX_SIZE")
		.map(|s| {
			s.parse::<u64>()
				.expect("LOG_MAX_SIZE must be a valid u64 if set")
		})
		.unwrap_or(1_073_741_824)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs::File;
	use std::io::Write;
	use tempfile::tempdir;

	#[test]
	fn test_strip_ansi_escapes() {
		let input = "\x1b[31mRed text\x1b[0m and \x1b[32mgreen text\x1b[0m";
		let expected = "Red text and green text";
		assert_eq!(strip_ansi_escapes(input), expected);
	}

	#[test]
	fn test_compute_rolled_file_path() {
		// Test with .log suffix
		let result = compute_rolled_file_path("app.log", "2023-01-01", 1);
		assert_eq!(result, "app-2023-01-01.1.log");

		// Test without .log suffix
		let result = compute_rolled_file_path("app", "2023-01-01", 2);
		assert_eq!(result, "app-2023-01-01.2.log");

		// Test with path
		let result = compute_rolled_file_path("logs/app.log", "2023-01-01", 3);
		assert_eq!(result, "logs/app-2023-01-01.3.log");
	}

	#[test]
	fn test_space_based_rolling() {
		// Create a temporary directory for our test files
		let dir = tempdir().expect("Failed to create temp directory");
		let base_path = dir.path().join("test.log").to_str().unwrap().to_string();
		let date_str = "2023-01-01";

		// Create an initial file that's larger than our max size
		let initial_path = compute_rolled_file_path(&base_path, date_str, 1);
		{
			let mut file = File::create(&initial_path).expect("Failed to create test file");
			// Write 100 bytes to the file
			file.write_all(&[0; 100])
				.expect("Failed to write to test file");
		}

		// Test with a max size of 50 bytes (our file is 100 bytes, so it should roll)
		let result = space_based_rolling(&initial_path, &base_path, date_str, 50);
		assert_eq!(result, compute_rolled_file_path(&base_path, date_str, 2));

		// Test with a max size of 200 bytes (our file is 100 bytes, so it should not roll)
		let result = space_based_rolling(&initial_path, &base_path, date_str, 200);
		assert_eq!(result, initial_path);
	}

	// This test checks if the LOG_MAX_SIZE environment variable is set to a valid u64 value.
	#[test]
	#[should_panic(expected = "LOG_MAX_SIZE must be a valid u64 if set")]
	fn test_invalid_log_max_size_panics() {
		std::env::set_var("LOG_MAX_SIZE", "not_a_number");
		let _ = parse_log_max_size(); // should panic here
	}
}
