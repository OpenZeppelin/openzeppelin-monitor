//! Security models
//!
//! This module contains the security models for the application.
//!
//! - `error`: Error types for security operations
//! - `secret`: Secret management and zeroization

mod error;
mod secret;

use std::env;

pub use error::{SecurityError, SecurityResult};
pub use secret::{SecretString, SecretValue};

pub fn get_env_var(key: &str) -> SecurityResult<String> {
	env::var(key).map_err(|e| {
		Box::new(SecurityError::parse_error(
			format!("Missing {} environment variable", key),
			Some(e.into()),
			None,
		))
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::env;

	#[test]
	fn test_get_env_var_success() {
		env::set_var("TEST_ENV_VAR", "test_value");
		let result = get_env_var("TEST_ENV_VAR");
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), "test_value".to_string());
		env::remove_var("TEST_ENV_VAR");
	}

	#[test]
	fn test_get_env_var_missing() {
		let result = get_env_var("NON_EXISTING_ENV_VAR");
		assert!(result.is_err());
		assert!(result
			.err()
			.unwrap()
			.to_string()
			.contains("Missing NON_EXISTING_ENV_VAR environment variable"));
	}
}
