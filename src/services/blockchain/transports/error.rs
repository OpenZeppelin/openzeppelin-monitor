//! Error types for blockchain transport services
//!
//! Provides error handling for network communication, JSON parsing, request serialization and URL rotation.

use crate::utils::logging::error::{ErrorContext, TraceableError};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
	/// HTTP error
	#[error("HTTP error: status {status_code} for URL {url}")]
	Http {
		status_code: reqwest::StatusCode,
		url: String,
		body: String,
		context: ErrorContext,
	},

	/// Network error
	#[error("Network error: {0}")]
	Network(ErrorContext),

	/// JSON parsing error
	#[error("Failed to parse JSON response: {0}")]
	ResponseParse(ErrorContext),

	/// Request body serialization error
	#[error("Failed to serialize request JSON: {0}")]
	RequestSerialization(ErrorContext),

	/// URL rotation error
	#[error("URL rotation failed: {0}")]
	UrlRotation(ErrorContext),
}

impl TransportError {
	pub fn http(
		status_code: reqwest::StatusCode,
		url: String,
		body: String,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		let msg = format!("HTTP error: status {} for URL {}", status_code, url);

		Self::Http {
			status_code,
			url,
			body,
			context: ErrorContext::new_with_log(msg, source, metadata),
		}
	}

	pub fn network(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::Network(ErrorContext::new_with_log(msg, source, metadata))
	}

	pub fn response_parse(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ResponseParse(ErrorContext::new_with_log(msg, source, metadata))
	}

	pub fn request_serialization(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::RequestSerialization(ErrorContext::new_with_log(msg, source, metadata))
	}
	pub fn url_rotation(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::UrlRotation(ErrorContext::new_with_log(msg, source, metadata))
	}
}

impl TraceableError for TransportError {
	fn trace_id(&self) -> String {
		match self {
			Self::Http { context, .. } => context.trace_id.clone(),
			Self::Network(ctx) => ctx.trace_id.clone(),
			Self::ResponseParse(ctx) => ctx.trace_id.clone(),
			Self::RequestSerialization(ctx) => ctx.trace_id.clone(),
			Self::UrlRotation(ctx) => ctx.trace_id.clone(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::{Error as IoError, ErrorKind};

	#[test]
	fn test_http_error_formatting() {
		let error = TransportError::http(
			reqwest::StatusCode::NOT_FOUND,
			"http://example.com".to_string(),
			"Not Found".to_string(),
			None,
			None,
		);
		assert_eq!(
			error.to_string(),
			"HTTP error: status 404 Not Found for URL http://example.com"
		);
	}

	#[test]
	fn test_network_error_formatting() {
		let error = TransportError::network("test error", None, None);
		assert_eq!(error.to_string(), "Network error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = TransportError::network(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Network error: test error [key1=value1]");
	}

	#[test]
	fn test_response_parse_error_formatting() {
		let error = TransportError::response_parse("test error", None, None);
		assert_eq!(
			error.to_string(),
			"Failed to parse JSON response: test error"
		);

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = TransportError::response_parse(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Failed to parse JSON response: test error [key1=value1]"
		);
	}

	#[test]
	fn test_request_serialization_error_formatting() {
		let error = TransportError::request_serialization("test error", None, None);
		assert_eq!(
			error.to_string(),
			"Failed to serialize request JSON: test error"
		);

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = TransportError::request_serialization(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Failed to serialize request JSON: test error [key1=value1]"
		);
	}

	#[test]
	fn test_url_rotation_error_formatting() {
		let error = TransportError::url_rotation("test error", None, None);
		assert_eq!(error.to_string(), "URL rotation failed: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = TransportError::url_rotation(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"URL rotation failed: test error [key1=value1]"
		);
	}

	#[test]
	fn test_error_source_chain() {
		let io_error = std::io::Error::new(std::io::ErrorKind::Other, "while reading config");

		let outer_error = TransportError::http(
			reqwest::StatusCode::INTERNAL_SERVER_ERROR,
			"http://example.com".to_string(),
			"Internal Server Error".to_string(),
			Some(Box::new(io_error)),
			None,
		);

		// Just test the string representation instead of the source chain
		assert!(outer_error.to_string().contains("Internal Server Error"));

		// For TransportError::Http, we know the implementation details
		if let TransportError::Http { context, .. } = &outer_error {
			// Check that the context has the right message
			assert_eq!(
				context.message,
				"HTTP error: status 500 Internal Server Error for URL http://example.com"
			);

			// Check that the context has the source error
			assert!(context.source.is_some());

			if let Some(src) = &context.source {
				assert_eq!(src.to_string(), "while reading config");
			}
		} else {
			panic!("Expected Http variant");
		}
	}

	#[test]
	fn test_trace_id_propagation() {
		// Create an error context with a known trace ID
		let error_context = ErrorContext::new("Inner error", None, None);
		let original_trace_id = error_context.trace_id.clone();

		// Wrap it in a TransportError
		let transport_error = TransportError::Http {
			status_code: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
			url: "http://example.com".to_string(),
			body: "Internal Server Error".to_string(),
			context: error_context,
		};

		// Verify the trace ID is preserved
		assert_eq!(transport_error.trace_id(), original_trace_id);

		// Test trace ID propagation through error chain
		let source_error = IoError::new(ErrorKind::Other, "Source error");
		let error_context = ErrorContext::new("Middle error", Some(Box::new(source_error)), None);
		let original_trace_id = error_context.trace_id.clone();

		let transport_error = TransportError::Http {
			status_code: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
			url: "http://example.com".to_string(),
			body: "Internal Server Error".to_string(),
			context: error_context,
		};
		assert_eq!(transport_error.trace_id(), original_trace_id);
	}
}
