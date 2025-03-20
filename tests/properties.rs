//! PBT tests for the OpenZeppelin Monitor.
//!
//! Contains tests for blockchain monitoring functionality across different
//! chains (EVM and Stellar) and mock implementations for testing.

mod properties {
	mod filters {
		mod evm {
			mod filter;
		}
		mod stellar {
			mod filter;
		}
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
	mod utils {
		mod executor;
	}
	mod strategies;
}
