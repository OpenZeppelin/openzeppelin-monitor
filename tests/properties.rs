//! PBT tests for the OpenZeppelin Monitor.
//!
//! Contains tests for blockchain monitoring functionality across different
//! chains (EVM and Stellar) and mock implementations for testing.

mod properties {
	mod filters {
		mod evm {
			mod address_evaluator;
			mod boolean_evaluator;
			mod filter;
			mod helpers;
			mod map_evaluator;
			mod numbers_evaluator;
			mod strings_evaluator;
		}
		mod stellar {
			mod evaluator;
			mod filter;
			mod helpers;
		}
	}
	mod notifications {
		mod email;
		mod payload_builder;
	}
	mod repositories {
		mod monitor;
		mod network;
		mod trigger;
	}
	mod triggers {
		mod script;
	}
	mod utils {
		mod logging;
	}
	mod strategies;
}
