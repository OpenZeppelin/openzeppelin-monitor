//! Integration tests for the OpenZeppelin Monitor.
//!
//! Contains tests for blockchain monitoring functionality across different
//! chains (EVM and Stellar) and mock implementations for testing.

mod integration {
	mod blockchain {
		mod pool;
		mod clients {
			mod evm {
				mod client;
			}
			mod stellar {
				mod client;
			}
		}
		mod transports {
			mod evm {
				mod http;
				mod transport;
			}
			mod stellar {
				mod http;
				mod transport;
			}
			mod endpoint_manager;
			mod http;
		}
	}
	mod bootstrap {
		mod main;
	}
	mod mocks;

	mod blockwatcher {
		mod service;
	}
	mod filters {
		pub mod common;
		mod evm {
			mod filter;
		}
		mod stellar {
			mod filter;
		}
	}
	mod notifications {
		mod email;
		mod script;
		mod webhook;
	}
	mod monitor {
		mod execution;
	}

	mod security {
		mod secret;
	}
}
