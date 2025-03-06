//! Logging utilities for the application
//!
//! This module provides utilities for setting up and configuring logging for the application.
//! It uses the `tracing_subscriber` crate to configure the logging.
//!
//! The `setup_logging` function sets up the logging for the application.
//! It uses the `tracing_subscriber` crate to configure the logging.
//! It sets the logging to stdout.
//!
//! The `setup_logging_with_writer` function sets up the logging for the application with a custom
//! writer. It uses the `tracing_subscriber` crate to configure the logging.
//! It sets the logging to a custom writer.
use tracing_subscriber::{filter::EnvFilter, fmt, prelude::*};

/// Setup logging for the application
///
/// This function sets up the logging for the application.
/// It uses the `tracing_subscriber` crate to configure the logging.
/// It sets the logging to stdout.
pub fn setup_logging() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
	setup_logging_with_writer(std::io::stdout)?;
	Ok(())
}

/// Setup logging for the application with a custom writer
///
/// This function sets up the logging for the application with a custom writer.
/// It uses the `tracing_subscriber` crate to configure the logging.
pub fn setup_logging_with_writer<W>(
	writer: W,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>
where
	W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + Send + Sync + 'static,
{
	// Create a filter based on environment variable or default to INFO
	let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

	// Create a subscriber that uses the filter and a console output
	tracing_subscriber::registry()
		.with(filter)
		.with(
			fmt::layer()
				.with_writer(writer)
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
		)
		.try_init()?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{
		io::Write,
		sync::{Arc, Mutex},
	};

	// Custom test writer that captures log output
	#[derive(Clone)]
	struct CaptureWriter {
		buffer: Arc<Mutex<Vec<u8>>>,
	}

	impl CaptureWriter {
		fn new() -> Self {
			Self {
				buffer: Arc::new(Mutex::new(Vec::new())),
			}
		}

		fn captured_output(&self) -> String {
			let buffer = self.buffer.lock().unwrap();
			String::from_utf8_lossy(&buffer).to_string()
		}
	}

	impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CaptureWriter {
		type Writer = Self;

		fn make_writer(&'a self) -> Self::Writer {
			self.clone()
		}
	}

	impl Write for CaptureWriter {
		fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
			let mut buffer = self.buffer.lock().unwrap();
			buffer.extend_from_slice(buf);
			Ok(buf.len())
		}

		fn flush(&mut self) -> std::io::Result<()> {
			Ok(())
		}
	}

	#[test]
	fn test_setup_logging() {
		let result = setup_logging();
		match result {
			Ok(_) => {}
			Err(e) => {
				// Check if the error is because a subscriber is already set
				let error_string = e.to_string();
				if !error_string.contains("a global default trace dispatcher has already been set")
				{
					panic!("Unexpected error setting up logging: {}", e);
				}
			}
		}
	}

	#[test]
	fn test_setup_logging_with_writer() {
		let writer = tracing_subscriber::fmt::TestWriter::default();

		let result = setup_logging_with_writer(writer);
		match result {
			Ok(_) => {}
			Err(e) => {
				let error_string = e.to_string();
				if !error_string.contains("a global default trace dispatcher has already been set")
				{
					panic!(
						"Unexpected error setting up logging with custom writer: {}",
						e
					);
				}
			}
		}
	}

	#[test]
	fn test_logging_filter_levels() {
		let original_var = std::env::var_os("RUST_LOG");
		std::env::set_var("RUST_LOG", "info");

		let writer = CaptureWriter::new();

		let result = setup_logging_with_writer(writer.clone());
		if result.is_err() {
			// Restore original environment
			match original_var {
				Some(val) => std::env::set_var("RUST_LOG", val),
				None => std::env::remove_var("RUST_LOG"),
			}
			return;
		}

		// Log messages at different levels
		tracing::trace!("This is a TRACE message"); // This should not be logged
		tracing::debug!("This is a DEBUG message"); // This should not be logged
		tracing::info!("This is an INFO message"); // This should be logged
		tracing::warn!("This is a WARN message"); // This should be logged
		tracing::error!("This is an ERROR message"); // This should be logged

		// Get the output
		let output = writer.captured_output();

		// Verify filtering worked correctly
		assert!(!output.contains("TRACE message"));
		assert!(!output.contains("DEBUG message"));
		assert!(output.contains("INFO message"));
		assert!(output.contains("WARN message"));
		assert!(output.contains("ERROR message"));

		// Restore original environment
		match original_var {
			Some(val) => std::env::set_var("RUST_LOG", val),
			None => std::env::remove_var("RUST_LOG"),
		}
	}
}
