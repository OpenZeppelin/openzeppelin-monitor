mod properties {
    // mod chain{
    //     mod evm;
    //     mod stellar;
    // }
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
