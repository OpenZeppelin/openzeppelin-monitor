//! Integration and PBT tests for the OpenZeppelin Monitor.
//!
//! Contains tests for blockchain monitoring functionality across different
//! chains (EVM and Stellar) and mock implementations for testing.

mod properties {
	mod filters {
		mod evm;
		mod stellar;
	}
	mod notifications {
		mod email;
		mod slack;
	}
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
