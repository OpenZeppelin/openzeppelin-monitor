//! Integration and PBT tests for the OpenZeppelin Monitor.
//!
//! Contains tests for blockchain monitoring functionality across different
//! chains (EVM and Stellar) and mock implementations for testing.

mod properties {
	mod matcher {
		mod evm;
		mod stellar;
	}
	mod notification;
	mod repositories {
		mod monitor;
		mod network;
		mod trigger;
	}
	mod strategies;
}

mod integration {
	mod mocks;
	mod filter {
		mod common;
		mod evm;
		mod stellar;
	}
}
