//! Integration and PBT tests for the OpenZeppelin Monitor.
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
				mod transport;
				mod web3;
			}
			mod stellar {
				mod horizon;
				mod soroban;
				mod transport;
			}
			mod endpoint_manager;
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
		mod discord;
		mod email;
		mod slack;
		mod webhook;
	}
}
