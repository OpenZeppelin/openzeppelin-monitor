//! Integration tests for the OpenZeppelin Monitor.
//!
//! Contains tests for blockchain monitoring functionality across different
//! chains and mock implementations for testing.

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
			mod midnight {
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
			mod midnight {
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
		mod midnight {
			mod filter;
		}
	}
	mod notifications {
		mod discord;
		mod email;
		mod script;
		mod slack;
		mod telegram;
		mod webhook;
	}
	mod monitor {
		mod execution;
	}
}
