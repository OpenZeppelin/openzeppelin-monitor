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
