use crate::models::{
	MidnightCallDetails, MidnightClaimMintDetails, MidnightDeploymentDetails, MidnightEvent,
	MidnightEventType, MidnightMaintainDetails, MidnightPayoutDetails, MidnightPhase,
	MidnightTopics, MidnightTxAppliedDetails,
};

/// A builder for creating test Midnight events with default values.
#[derive(Debug)]
pub struct EventBuilder {
	event: MidnightEvent,
}

impl Default for EventBuilder {
	/// Default event builder with a testnet transaction hash
	fn default() -> Self {
		Self {
			event: MidnightEvent::from(MidnightEventType::Unknown(
				"Unknown event type".to_string(),
			)),
		}
	}
}

impl EventBuilder {
	/// Creates a new EventBuilder instance.
	pub fn new() -> Self {
		Self::default()
	}

	/// Sets the event type directly.
	pub fn event_type(mut self, event_type: MidnightEventType) -> Self {
		self.event = MidnightEvent::from(event_type);
		self
	}

	/// Creates a transaction applied event.
	pub fn tx_applied(mut self, tx_hash: String) -> Self {
		self.event = MidnightEvent::from(MidnightEventType::MidnightTxApplied(
			MidnightTxAppliedDetails {
				phase: MidnightPhase::default(),
				topics: MidnightTopics::default(),
				tx_hash,
			},
		));
		self
	}

	/// Creates an only guaranteed transaction applied event.
	pub fn only_guaranteed_tx_applied(mut self, tx_hash: String) -> Self {
		self.event = MidnightEvent::from(MidnightEventType::MidnightOnlyGuaranteedTxApplied(
			MidnightTxAppliedDetails {
				phase: MidnightPhase::default(),
				topics: MidnightTopics::default(),
				tx_hash,
			},
		));
		self
	}

	/// Creates a contract call event.
	pub fn call_contract(mut self, address: String, tx_hash: String) -> Self {
		self.event = MidnightEvent::from(MidnightEventType::MidnightCallContract(
			MidnightCallDetails {
				phase: MidnightPhase::default(),
				topics: MidnightTopics::default(),
				address,
				tx_hash,
			},
		));
		self
	}

	/// Creates a contract deployment event.
	pub fn deploy_contract(mut self, address: String, tx_hash: String) -> Self {
		self.event = MidnightEvent::from(MidnightEventType::MidnightDeployContract(
			MidnightDeploymentDetails {
				phase: MidnightPhase::default(),
				topics: MidnightTopics::default(),
				address,
				tx_hash,
			},
		));
		self
	}

	/// Creates a contract maintenance event.
	pub fn maintain_contract(mut self, address: String, tx_hash: String) -> Self {
		self.event = MidnightEvent::from(MidnightEventType::MidnightMaintainContract(
			MidnightMaintainDetails {
				phase: MidnightPhase::default(),
				topics: MidnightTopics::default(),
				address,
				tx_hash,
			},
		));
		self
	}

	/// Creates a payout minted event.
	pub fn payout_minted(mut self, amount: u128, receiver: String) -> Self {
		self.event = MidnightEvent::from(MidnightEventType::MidnightPayoutMinted(
			MidnightPayoutDetails {
				phase: MidnightPhase::default(),
				topics: MidnightTopics::default(),
				amount,
				receiver,
			},
		));
		self
	}

	/// Creates a claim mint event.
	pub fn claim_mint(mut self, coin_type: String, value: u128, tx_hash: String) -> Self {
		self.event = MidnightEvent::from(MidnightEventType::MidnightClaimMint(
			MidnightClaimMintDetails {
				phase: MidnightPhase::default(),
				topics: MidnightTopics::default(),
				coin_type,
				value,
				tx_hash,
			},
		));
		self
	}

	/// Sets custom topics for the event.
	pub fn topics(mut self, topics: Vec<String>) -> Self {
		match &mut self.event {
			MidnightEvent(event_type) => match event_type {
				MidnightEventType::MidnightTxApplied(details) => {
					details.topics = MidnightTopics { topics }
				}
				MidnightEventType::MidnightOnlyGuaranteedTxApplied(details) => {
					details.topics = MidnightTopics { topics }
				}
				MidnightEventType::MidnightCallContract(details) => {
					details.topics = MidnightTopics { topics }
				}
				MidnightEventType::MidnightDeployContract(details) => {
					details.topics = MidnightTopics { topics }
				}
				MidnightEventType::MidnightMaintainContract(details) => {
					details.topics = MidnightTopics { topics }
				}
				MidnightEventType::MidnightPayoutMinted(details) => {
					details.topics = MidnightTopics { topics }
				}
				MidnightEventType::MidnightClaimMint(details) => {
					details.topics = MidnightTopics { topics }
				}
				MidnightEventType::Unknown(_) => (),
			},
		}
		self
	}

	/// Sets custom phase for the event.
	pub fn phase(mut self, phase: MidnightPhase) -> Self {
		match &mut self.event {
			MidnightEvent(event_type) => match event_type {
				MidnightEventType::MidnightTxApplied(details) => details.phase = phase,
				MidnightEventType::MidnightOnlyGuaranteedTxApplied(details) => {
					details.phase = phase
				}
				MidnightEventType::MidnightCallContract(details) => details.phase = phase,
				MidnightEventType::MidnightDeployContract(details) => details.phase = phase,
				MidnightEventType::MidnightMaintainContract(details) => details.phase = phase,
				MidnightEventType::MidnightPayoutMinted(details) => details.phase = phase,
				MidnightEventType::MidnightClaimMint(details) => details.phase = phase,
				MidnightEventType::Unknown(_) => (),
			},
		}
		self
	}

	/// Builds the Event instance.
	pub fn build(self) -> MidnightEvent {
		self.event
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_builder_default() {
		let event = EventBuilder::new().build();
		match &*event {
			MidnightEventType::Unknown(_) => (),
			_ => panic!("Expected Unknown event type"),
		}
	}

	#[test]
	fn test_builder_tx_applied() {
		let tx_hash = "0x123".to_string();
		let event = EventBuilder::new().tx_applied(tx_hash.clone()).build();
		assert!(event.is_tx_applied());
		assert_eq!(event.get_tx_hash(), Some(tx_hash));
	}

	#[test]
	fn test_builder_only_guaranteed_tx_applied() {
		let tx_hash = "0x123".to_string();
		let event = EventBuilder::new()
			.only_guaranteed_tx_applied(tx_hash.clone())
			.build();
		assert!(event.is_only_guaranteed_tx_applied());
		assert_eq!(event.get_tx_hash(), Some(tx_hash));
	}

	#[test]
	fn test_builder_call_contract() {
		let address = "0x123".to_string();
		let tx_hash = "0x456".to_string();
		let event = EventBuilder::new()
			.call_contract(address.clone(), tx_hash.clone())
			.build();
		assert_eq!(event.get_tx_hash(), Some(tx_hash));
	}

	#[test]
	fn test_builder_deploy_contract() {
		let address = "0x123".to_string();
		let tx_hash = "0x456".to_string();
		let event = EventBuilder::new()
			.deploy_contract(address.clone(), tx_hash.clone())
			.build();
		assert_eq!(event.get_tx_hash(), Some(tx_hash));
	}

	#[test]
	fn test_builder_maintain_contract() {
		let address = "0x123".to_string();
		let tx_hash = "0x456".to_string();
		let event = EventBuilder::new()
			.maintain_contract(address.clone(), tx_hash.clone())
			.build();
		assert_eq!(event.get_tx_hash(), Some(tx_hash));
	}

	#[test]
	fn test_builder_payout_minted() {
		let amount = 100u128;
		let receiver = "0x123".to_string();
		let event = EventBuilder::new()
			.payout_minted(amount, receiver.clone())
			.build();
		assert_eq!(event.get_tx_hash(), None);
	}

	#[test]
	fn test_builder_claim_mint() {
		let coin_type = "ETH".to_string();
		let value = 100u128;
		let tx_hash = "0x123".to_string();
		let event = EventBuilder::new()
			.claim_mint(coin_type.clone(), value, tx_hash.clone())
			.build();
		assert_eq!(event.get_tx_hash(), Some(tx_hash));
	}

	#[test]
	fn test_builder_with_topics() {
		let topics = vec!["topic1".to_string(), "topic2".to_string()];
		let event = EventBuilder::new()
			.tx_applied("0x123".to_string())
			.topics(topics.clone())
			.build();
		match &*event {
			MidnightEventType::MidnightTxApplied(details) => {
				assert_eq!(details.topics, MidnightTopics { topics });
			}
			_ => panic!("Expected MidnightTxApplied event type"),
		}
	}

	#[test]
	fn test_builder_with_phase() {
		let phase = MidnightPhase::ApplyExtrinsic(1);
		let event = EventBuilder::new()
			.tx_applied("0x123".to_string())
			.phase(phase.clone())
			.build();
		match &*event {
			MidnightEventType::MidnightTxApplied(details) => {
				assert_eq!(details.phase, phase);
			}
			_ => panic!("Expected MidnightTxApplied event type"),
		}
	}
}
