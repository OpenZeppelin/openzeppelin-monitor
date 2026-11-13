//! Test helper utilities
//!
//! This module contains test helper utilities for the application.
//!
//! - `builders`: Test helper utilities for creating test instances of models
//! - `fs`: Test helper utilities for creating files (Unix unit tests only)
//! - `http`: Test helper utilities for creating HTTP clients

pub mod builders {
	// Chain specific test helpers
	pub mod evm {
		pub mod monitor;
		pub mod monitor_match;
		pub mod receipt;
		pub mod transaction;
	}
	pub mod stellar {
		pub mod monitor;
	}

	pub mod midnight {
		pub mod block;
		pub mod event;
		pub mod monitor;
		pub mod transaction;
	}

	pub mod solana {
		pub mod block;
		pub mod monitor;
		pub mod transaction;
	}

	// Chain agnostic test helpers
	pub mod network;
	pub mod trigger;
}

#[cfg(all(test, unix))]
mod fs;
pub mod http;

pub use builders::*;
#[cfg(all(test, unix))]
pub(crate) use fs::create_file_with_mode;
pub use http::*;
