//! Midnight event data structures.
//!
//! This module defines the event types and structures used in the Midnight blockchain.
//! Events are emitted during transaction execution and represent various state changes
//! in the blockchain, such as contract deployments, calls, and transaction applications.
//!
//! The structures are based on the Midnight Node implementation:
//! <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/src/lib.rs#L149-L205>
//! <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/runtime/src/model.rs#L26-L187>

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::ops::{Deref, DerefMut};

/// Represents the phase of a blockchain event.
///
/// Events can occur during different phases of block processing:
/// - During extrinsic application (with the extrinsic index)
/// - During block finalization
/// - During block initialization
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Phase {
	/// Event occurred during extrinsic application.
	/// The associated value is the index of the extrinsic in the block.
	ApplyExtrinsic(u32),
	/// Event occurred during block finalization.
	Finalization,
	/// Event occurred during block initialization.
	Initialization,
}

impl Default for Phase {
	fn default() -> Self {
		Self::ApplyExtrinsic(0)
	}
}

/// Contains a list of topics associated with an event.
///
/// Topics are used for event filtering and indexing in the blockchain.
/// Each topic is a string that can be used to categorize or filter events.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub struct Topics {
	/// List of topic strings associated with the event.
	pub topics: Vec<String>,
}

/// Details of a transaction that has been applied to the blockchain.
///
/// This structure contains information about a transaction that has been
/// successfully processed and included in a block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TxAppliedDetails {
	/// The phase during which the transaction was applied.
	pub phase: Phase,
	/// Topics associated with the transaction application.
	pub topics: Topics,
	/// The hash of the applied transaction.
	pub tx_hash: String,
}

/// Details of a contract maintenance operation.
///
/// This structure contains information about a contract ownership change
/// that enables SNARK upgrades or other maintenance operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MaintainDetails {
	/// The phase during which the maintenance operation occurred.
	pub phase: Phase,
	/// Topics associated with the maintenance operation.
	pub topics: Topics,
	/// The address of the contract being maintained.
	pub address: String,
	/// The hash of the transaction that performed the maintenance.
	pub tx_hash: String,
}

/// Details of a contract deployment.
///
/// This structure contains information about a newly deployed contract,
/// including its address and the transaction that created it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DeploymentDetails {
	/// The phase during which the contract was deployed.
	pub phase: Phase,
	/// Topics associated with the contract deployment.
	pub topics: Topics,
	/// The address of the newly deployed contract.
	pub address: String,
	/// The hash of the transaction that deployed the contract.
	pub tx_hash: String,
}

/// Details of a contract call.
///
/// This structure contains information about a function call to a contract,
/// including the contract address and the transaction that made the call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CallDetails {
	/// The phase during which the contract call occurred.
	pub phase: Phase,
	/// Topics associated with the contract call.
	pub topics: Topics,
	/// The address of the contract being called.
	pub address: String,
	/// The hash of the transaction that made the call.
	pub tx_hash: String,
}

/// Details of a mint claim operation.
///
/// This structure contains information about a claim for minted tokens,
/// including the coin type, amount, and the transaction that made the claim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ClaimMintDetails {
	/// The phase during which the mint claim occurred.
	pub phase: Phase,
	/// Topics associated with the mint claim.
	pub topics: Topics,
	/// The type of coin being claimed.
	pub coin_type: String,
	/// The amount of tokens being claimed.
	pub value: u128,
	/// The hash of the transaction that made the claim.
	pub tx_hash: String,
}

/// Details of a payout operation.
///
/// This structure contains information about a payout of tokens,
/// including the amount and the recipient's address.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PayoutDetails {
	/// The phase during which the payout occurred.
	pub phase: Phase,
	/// Topics associated with the payout.
	pub topics: Topics,
	/// The amount of tokens being paid out.
	pub amount: u128,
	/// The address of the recipient.
	pub receiver: String,
}

/// Enum representing different types of events that can occur in the Midnight blockchain.
///
/// Each variant contains specific details about the event type, such as contract calls,
/// deployments, or transaction applications. This enum is used to categorize and process
/// different types of blockchain events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
	/// A contract was called.
	/// Contains details about the contract call, including the contract address and transaction hash.
	MidnightCallContract(CallDetails),
	/// A contract has been deployed.
	/// Contains details about the contract deployment, including the new contract address and transaction hash.
	MidnightDeployContract(DeploymentDetails),
	/// A transaction has been applied (both the guaranteed and conditional part).
	/// Contains details about the fully applied transaction.
	MidnightTxApplied(TxAppliedDetails),
	/// Only guaranteed transactions have been applied.
	/// Contains details about the partially applied transaction (guaranteed part only).
	MidnightOnlyGuaranteedTxApplied(TxAppliedDetails),
	/// Contract ownership changes to enable snark upgrades.
	/// Contains details about the contract maintenance operation.
	MidnightMaintainContract(MaintainDetails),
	/// New payout minted.
	/// Contains details about a new token payout, including amount and recipient.
	MidnightPayoutMinted(PayoutDetails),
	/// Payout was claimed.
	/// Contains details about a claim for minted tokens.
	MidnightClaimMint(ClaimMintDetails),
	/// Unknown event type.
	/// A default variant for when the event type is not known or not Midnight specific.
	/// Contains details about an unknown event type.
	Unknown(String),
}

impl Default for EventType {
	fn default() -> Self {
		Self::Unknown(format!(
			"Unknown event type: {}",
			std::any::type_name::<Self>()
		))
	}
}

/// Wrapper around EventType that provides additional functionality.
///
/// This type implements convenience methods for working with Midnight events
/// while maintaining compatibility with the RPC response format. It serves as the
/// primary interface for handling events in the Midnight blockchain.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Event(pub EventType);

/// Additional methods for Event
impl Event {
	/// Check if the event is a transaction applied event.
	pub fn is_tx_applied(&self) -> bool {
		matches!(self.0, EventType::MidnightTxApplied(_))
	}

	/// Check if the event is a transaction applied event.
	pub fn is_only_guaranteed_tx_applied(&self) -> bool {
		matches!(self.0, EventType::MidnightOnlyGuaranteedTxApplied(_))
	}

	/// Check if the event is a success event.
	///
	/// This is a convenience method that checks if the event is a transaction applied event
	/// or a only guaranteed transaction applied event.
	pub fn is_success(&self) -> bool {
		self.is_tx_applied() || self.is_only_guaranteed_tx_applied()
	}

	/// Get the transaction hash from the event.
	///
	/// This method returns the transaction hash from the event.
	pub fn get_tx_hash(&self) -> Option<String> {
		match &self.0 {
			EventType::MidnightTxApplied(details) => Some(details.tx_hash.clone()),
			EventType::MidnightOnlyGuaranteedTxApplied(details) => Some(details.tx_hash.clone()),
			EventType::MidnightCallContract(details) => Some(details.tx_hash.clone()),
			EventType::MidnightDeployContract(details) => Some(details.tx_hash.clone()),
			EventType::MidnightMaintainContract(details) => Some(details.tx_hash.clone()),
			EventType::MidnightClaimMint(details) => Some(details.tx_hash.clone()),
			EventType::MidnightPayoutMinted(_) => None,
			EventType::Unknown(_) => None,
		}
	}

	/// Get the topics from the event.
	///
	/// This method returns the topics from the event.
	pub fn get_topics(&self) -> Option<Vec<String>> {
		match &self.0 {
			EventType::MidnightTxApplied(details) => Some(details.topics.topics.clone()),
			EventType::MidnightOnlyGuaranteedTxApplied(details) => {
				Some(details.topics.topics.clone())
			}
			EventType::MidnightCallContract(details) => Some(details.topics.topics.clone()),
			EventType::MidnightDeployContract(details) => Some(details.topics.topics.clone()),
			EventType::MidnightMaintainContract(details) => Some(details.topics.topics.clone()),
			EventType::MidnightPayoutMinted(details) => Some(details.topics.topics.clone()),
			EventType::MidnightClaimMint(details) => Some(details.topics.topics.clone()),
			EventType::Unknown(_) => None,
		}
	}

	/// Get the phase from the event.
	///
	/// This method returns the phase from the event.
	pub fn get_phase(&self) -> Option<Phase> {
		match &self.0 {
			EventType::MidnightTxApplied(details) => Some(details.phase.clone()),
			EventType::MidnightOnlyGuaranteedTxApplied(details) => Some(details.phase.clone()),
			EventType::MidnightCallContract(details) => Some(details.phase.clone()),
			EventType::MidnightDeployContract(details) => Some(details.phase.clone()),
			EventType::MidnightMaintainContract(details) => Some(details.phase.clone()),
			EventType::MidnightPayoutMinted(details) => Some(details.phase.clone()),
			EventType::MidnightClaimMint(details) => Some(details.phase.clone()),
			EventType::Unknown(_) => None,
		}
	}
}

/// Dereference the EventType
impl Deref for Event {
	type Target = EventType;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl DerefMut for Event {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

impl From<EventType> for Event {
	fn from(event_type: EventType) -> Self {
		Self(event_type)
	}
}

impl From<Event> for EventType {
	fn from(event: Event) -> Self {
		event.0
	}
}

impl From<Value> for Event {
	fn from(value: Value) -> Self {
		match serde_json::from_value::<EventType>(value) {
			Ok(event_type) => Event(event_type),
			Err(e) => Event(EventType::Unknown(format!(
				"Failed to deserialize event: {}",
				e.to_string().split(",").next().unwrap_or_default()
			))),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn test_tx_applied_details() {
		let details = TxAppliedDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		assert_eq!(
			details.tx_hash,
			"0x0000000000000000000000000000000000000000000000000000000000000000"
		);
	}

	#[test]
	fn test_maintain_details() {
		let details = MaintainDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			address: "0x123".to_string(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		assert_eq!(
			details.tx_hash,
			"0x0000000000000000000000000000000000000000000000000000000000000000"
		);
		assert_eq!(details.address, "0x123");
	}

	#[test]
	fn test_deployment_details() {
		let details = DeploymentDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			address: "0x123".to_string(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		assert_eq!(
			details.tx_hash,
			"0x0000000000000000000000000000000000000000000000000000000000000000"
		);
		assert_eq!(details.address, "0x123");
	}

	#[test]
	fn test_call_details() {
		let details = CallDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			address: "0x123".to_string(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		assert_eq!(
			details.tx_hash,
			"0x0000000000000000000000000000000000000000000000000000000000000000"
		);
		assert_eq!(details.address, "0x123");
	}

	#[test]
	fn test_claim_mint_details() {
		let details = ClaimMintDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			coin_type: "0x123".to_string(),
			value: 100u128,
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		assert_eq!(
			details.tx_hash,
			"0x0000000000000000000000000000000000000000000000000000000000000000"
		);
		assert_eq!(details.coin_type, "0x123");
		assert_eq!(details.value, 100u128);
	}

	#[test]
	fn test_payout_details() {
		let details = PayoutDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			amount: 100u128,
			receiver: "0x123".to_string(),
		};
		assert_eq!(details.amount, 100u128);
		assert_eq!(details.receiver, "0x123");
	}

	#[test]
	fn test_event_type_contract_call() {
		let call_details = CallDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			address: "0x123".to_string(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event_type = EventType::MidnightCallContract(call_details);

		match event_type {
			EventType::MidnightCallContract(details) => {
				assert_eq!(
					details.tx_hash,
					"0x0000000000000000000000000000000000000000000000000000000000000000"
				);
				assert_eq!(details.address, "0x123");
			}
			_ => panic!("Expected MidnightCallContract event type"),
		}
	}

	#[test]
	fn test_event_type_contract_deploy() {
		let deploy_details = DeploymentDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			address: "0x123".to_string(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event_type = EventType::MidnightDeployContract(deploy_details);

		match event_type {
			EventType::MidnightDeployContract(details) => {
				assert_eq!(
					details.tx_hash,
					"0x0000000000000000000000000000000000000000000000000000000000000000"
				);
				assert_eq!(details.address, "0x123");
			}
			_ => panic!("Expected MidnightDeployContract event type"),
		}
	}

	#[test]
	fn test_event_type_tx_applied() {
		let tx_details = TxAppliedDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event_type = EventType::MidnightTxApplied(tx_details);

		match event_type {
			EventType::MidnightTxApplied(details) => {
				assert_eq!(
					details.tx_hash,
					"0x0000000000000000000000000000000000000000000000000000000000000000"
				);
			}
			_ => panic!("Expected MidnightTxApplied event type"),
		}
	}

	#[test]
	fn test_event_deref() {
		let tx_details = TxAppliedDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event_type = EventType::MidnightTxApplied(tx_details);
		let event = Event(event_type);

		match &*event {
			EventType::MidnightTxApplied(details) => {
				assert_eq!(
					details.tx_hash,
					"0x0000000000000000000000000000000000000000000000000000000000000000"
				);
			}
			_ => panic!("Expected MidnightTxApplied event type"),
		}
	}

	#[test]
	fn test_event_from_event_type() {
		let tx_details = TxAppliedDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event_type = EventType::MidnightTxApplied(tx_details);
		let event = Event::from(event_type);

		match &*event {
			EventType::MidnightTxApplied(details) => {
				assert_eq!(
					details.tx_hash,
					"0x0000000000000000000000000000000000000000000000000000000000000000"
				);
			}
			_ => panic!("Expected MidnightTxApplied event type"),
		}
	}

	#[test]
	fn test_event_type_from_event() {
		let tx_details = TxAppliedDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event_type = EventType::MidnightTxApplied(tx_details);
		let event = Event(event_type);
		let converted_event_type = EventType::from(event);

		match converted_event_type {
			EventType::MidnightTxApplied(details) => {
				assert_eq!(
					details.tx_hash,
					"0x0000000000000000000000000000000000000000000000000000000000000000"
				);
			}
			_ => panic!("Expected MidnightTxApplied event type"),
		}
	}

	#[test]
	fn test_event_serialization() {
		let tx_details = TxAppliedDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event_type = EventType::MidnightTxApplied(tx_details);
		let event = Event(event_type);

		let serialized = serde_json::to_string(&event).unwrap();
		let deserialized: Event = serde_json::from_str(&serialized).unwrap();

		match &*deserialized {
			EventType::MidnightTxApplied(details) => {
				assert_eq!(
					details.tx_hash,
					"0x0000000000000000000000000000000000000000000000000000000000000000"
				);
			}
			_ => panic!("Expected MidnightTxApplied event type"),
		}
	}

	#[test]
	fn test_event_from_value() {
		// Test valid TxApplied event
		let valid_json = json!({
			"MidnightTxApplied": {
				"phase": {
					"ApplyExtrinsic": 0
				},
				"topics": { "topics": [] },
				"tx_hash": "0x0000000000000000000000000000000000000000000000000000000000000000"
			}
		});
		let event = Event::from(valid_json);
		match &*event {
			EventType::MidnightTxApplied(details) => {
				assert_eq!(
					details.tx_hash,
					"0x0000000000000000000000000000000000000000000000000000000000000000"
				);
				assert_eq!(details.phase, Phase::ApplyExtrinsic(0));
				assert_eq!(details.topics.topics, Vec::<String>::new());
			}
			_ => panic!("Expected MidnightTxApplied event type"),
		}

		// Test invalid event type
		let invalid_json = json!({
			"InvalidType": {
				"some_field": "value"
			}
		});
		let event = Event::from(invalid_json);
		match &*event {
			EventType::Unknown(_) => (),
			_ => panic!("Expected Unknown event type"),
		}

		// Test malformed JSON
		let malformed_json = json!({
			"MidnightTxApplied": "not_an_object"
		});
		let event = Event::from(malformed_json);
		match &*event {
			EventType::Unknown(_) => (),
			_ => panic!("Expected Unknown event type"),
		}
	}

	#[test]
	fn test_event_get_tx_hash() {
		let tx_details = TxAppliedDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event_type = EventType::MidnightTxApplied(tx_details);
		let event = Event(event_type);
		assert_eq!(
			event.get_tx_hash(),
			Some("0x0000000000000000000000000000000000000000000000000000000000000000".to_string())
		);

		let call_details = CallDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			address: "0x123".to_string(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event_type = EventType::MidnightCallContract(call_details);
		let event = Event(event_type);
		assert_eq!(
			event.get_tx_hash(),
			Some("0x0000000000000000000000000000000000000000000000000000000000000000".to_string())
		);

		let deploy_details = DeploymentDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			address: "0x123".to_string(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event_type = EventType::MidnightDeployContract(deploy_details);
		let event = Event(event_type);
		assert_eq!(
			event.get_tx_hash(),
			Some("0x0000000000000000000000000000000000000000000000000000000000000000".to_string())
		);

		let maintain_details = MaintainDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			address: "0x123".to_string(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event_type = EventType::MidnightMaintainContract(maintain_details);
		let event = Event(event_type);
		assert_eq!(
			event.get_tx_hash(),
			Some("0x0000000000000000000000000000000000000000000000000000000000000000".to_string())
		);

		let claim_mint_details = ClaimMintDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			coin_type: "0x123".to_string(),
			value: 100u128,
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event_type = EventType::MidnightClaimMint(claim_mint_details);
		let event = Event(event_type);
		assert_eq!(
			event.get_tx_hash(),
			Some("0x0000000000000000000000000000000000000000000000000000000000000000".to_string())
		);

		let payout_details = PayoutDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			amount: 100u128,
			receiver: "0x123".to_string(),
		};
		let event_type = EventType::MidnightPayoutMinted(payout_details);
		let event = Event(event_type);
		assert_eq!(event.get_tx_hash(), None);

		let unknown_event = Event(EventType::Unknown("unknown".to_string()));
		assert_eq!(unknown_event.get_tx_hash(), None);
	}

	#[test]
	fn test_event_type_checks() {
		// Test is_tx_applied
		let tx_details = TxAppliedDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event = Event(EventType::MidnightTxApplied(tx_details));
		assert!(event.is_tx_applied());
		assert!(!event.is_only_guaranteed_tx_applied());
		assert!(event.is_success());

		// Test is_only_guaranteed_tx_applied
		let tx_details = TxAppliedDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event = Event(EventType::MidnightOnlyGuaranteedTxApplied(tx_details));
		assert!(!event.is_tx_applied());
		assert!(event.is_only_guaranteed_tx_applied());
		assert!(event.is_success());

		// Test other event types
		let call_details = CallDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			address: "0x123".to_string(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event = Event(EventType::MidnightCallContract(call_details));
		assert!(!event.is_tx_applied());
		assert!(!event.is_only_guaranteed_tx_applied());
		assert!(!event.is_success());

		let deploy_details = DeploymentDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			address: "0x123".to_string(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event = Event(EventType::MidnightDeployContract(deploy_details));
		assert!(!event.is_tx_applied());
		assert!(!event.is_only_guaranteed_tx_applied());
		assert!(!event.is_success());

		let maintain_details = MaintainDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			address: "0x123".to_string(),
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event = Event(EventType::MidnightMaintainContract(maintain_details));
		assert!(!event.is_tx_applied());
		assert!(!event.is_only_guaranteed_tx_applied());
		assert!(!event.is_success());

		let payout_details = PayoutDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			amount: 100u128,
			receiver: "0x123".to_string(),
		};
		let event = Event(EventType::MidnightPayoutMinted(payout_details));
		assert!(!event.is_tx_applied());
		assert!(!event.is_only_guaranteed_tx_applied());
		assert!(!event.is_success());

		let claim_mint_details = ClaimMintDetails {
			phase: Phase::default(),
			topics: Topics::default(),
			coin_type: "0x123".to_string(),
			value: 100u128,
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
		};
		let event = Event(EventType::MidnightClaimMint(claim_mint_details));
		assert!(!event.is_tx_applied());
		assert!(!event.is_only_guaranteed_tx_applied());
		assert!(!event.is_success());

		let event = Event(EventType::Unknown("unknown".to_string()));
		assert!(!event.is_tx_applied());
		assert!(!event.is_only_guaranteed_tx_applied());
		assert!(!event.is_success());
	}

	#[test]
	fn test_event_get_topics() {
		let topics = vec!["topic1".to_string(), "topic2".to_string()];
		let topics_struct = Topics {
			topics: topics.clone(),
		};

		// Test TxApplied
		let tx_details = TxAppliedDetails {
			phase: Phase::default(),
			topics: topics_struct.clone(),
			tx_hash: "0x123".to_string(),
		};
		let event = Event(EventType::MidnightTxApplied(tx_details));
		assert_eq!(event.get_topics(), Some(topics.clone()));

		// Test OnlyGuaranteedTxApplied
		let tx_details = TxAppliedDetails {
			phase: Phase::default(),
			topics: topics_struct.clone(),
			tx_hash: "0x123".to_string(),
		};
		let event = Event(EventType::MidnightOnlyGuaranteedTxApplied(tx_details));
		assert_eq!(event.get_topics(), Some(topics.clone()));

		// Test CallContract
		let call_details = CallDetails {
			phase: Phase::default(),
			topics: topics_struct.clone(),
			address: "0x123".to_string(),
			tx_hash: "0x456".to_string(),
		};
		let event = Event(EventType::MidnightCallContract(call_details));
		assert_eq!(event.get_topics(), Some(topics.clone()));

		// Test DeployContract
		let deploy_details = DeploymentDetails {
			phase: Phase::default(),
			topics: topics_struct.clone(),
			address: "0x123".to_string(),
			tx_hash: "0x456".to_string(),
		};
		let event = Event(EventType::MidnightDeployContract(deploy_details));
		assert_eq!(event.get_topics(), Some(topics.clone()));

		// Test MaintainContract
		let maintain_details = MaintainDetails {
			phase: Phase::default(),
			topics: topics_struct.clone(),
			address: "0x123".to_string(),
			tx_hash: "0x456".to_string(),
		};
		let event = Event(EventType::MidnightMaintainContract(maintain_details));
		assert_eq!(event.get_topics(), Some(topics.clone()));

		// Test PayoutMinted
		let payout_details = PayoutDetails {
			phase: Phase::default(),
			topics: topics_struct.clone(),
			amount: 100u128,
			receiver: "0x123".to_string(),
		};
		let event = Event(EventType::MidnightPayoutMinted(payout_details));
		assert_eq!(event.get_topics(), Some(topics.clone()));

		// Test ClaimMint
		let claim_mint_details = ClaimMintDetails {
			phase: Phase::default(),
			topics: topics_struct,
			coin_type: "ETH".to_string(),
			value: 100u128,
			tx_hash: "0x456".to_string(),
		};
		let event = Event(EventType::MidnightClaimMint(claim_mint_details));
		assert_eq!(event.get_topics(), Some(topics));

		// Test Unknown
		let event = Event(EventType::Unknown("unknown".to_string()));
		assert_eq!(event.get_topics(), None);
	}

	#[test]
	fn test_event_get_phase() {
		let phase = Phase::ApplyExtrinsic(1);

		// Test TxApplied
		let tx_details = TxAppliedDetails {
			phase: phase.clone(),
			topics: Topics::default(),
			tx_hash: "0x123".to_string(),
		};
		let event = Event(EventType::MidnightTxApplied(tx_details));
		assert_eq!(event.get_phase(), Some(phase.clone()));

		// Test OnlyGuaranteedTxApplied
		let tx_details = TxAppliedDetails {
			phase: phase.clone(),
			topics: Topics::default(),
			tx_hash: "0x123".to_string(),
		};
		let event = Event(EventType::MidnightOnlyGuaranteedTxApplied(tx_details));
		assert_eq!(event.get_phase(), Some(phase.clone()));

		// Test CallContract
		let call_details = CallDetails {
			phase: phase.clone(),
			topics: Topics::default(),
			address: "0x123".to_string(),
			tx_hash: "0x456".to_string(),
		};
		let event = Event(EventType::MidnightCallContract(call_details));
		assert_eq!(event.get_phase(), Some(phase.clone()));

		// Test DeployContract
		let deploy_details = DeploymentDetails {
			phase: phase.clone(),
			topics: Topics::default(),
			address: "0x123".to_string(),
			tx_hash: "0x456".to_string(),
		};
		let event = Event(EventType::MidnightDeployContract(deploy_details));
		assert_eq!(event.get_phase(), Some(phase.clone()));

		// Test MaintainContract
		let maintain_details = MaintainDetails {
			phase: phase.clone(),
			topics: Topics::default(),
			address: "0x123".to_string(),
			tx_hash: "0x456".to_string(),
		};
		let event = Event(EventType::MidnightMaintainContract(maintain_details));
		assert_eq!(event.get_phase(), Some(phase.clone()));

		// Test PayoutMinted
		let payout_details = PayoutDetails {
			phase: phase.clone(),
			topics: Topics::default(),
			amount: 100u128,
			receiver: "0x123".to_string(),
		};
		let event = Event(EventType::MidnightPayoutMinted(payout_details));
		assert_eq!(event.get_phase(), Some(phase.clone()));

		// Test ClaimMint
		let claim_mint_details = ClaimMintDetails {
			phase: phase.clone(),
			topics: Topics::default(),
			coin_type: "ETH".to_string(),
			value: 100u128,
			tx_hash: "0x456".to_string(),
		};
		let event = Event(EventType::MidnightClaimMint(claim_mint_details));
		assert_eq!(event.get_phase(), Some(phase));

		// Test Unknown
		let event = Event(EventType::Unknown("unknown".to_string()));
		assert_eq!(event.get_phase(), None);
	}

	#[test]
	fn test_event_type_default() {
		let default_event_type = EventType::default();
		match default_event_type {
			EventType::Unknown(message) => {
				assert!(message.starts_with("Unknown event type: "));
				assert!(message.contains("EventType"));
			}
			_ => panic!("Expected Unknown event type"),
		}
	}

	#[test]
	fn test_event_deref_mut() {
		let mut event = Event(EventType::Unknown("original".to_string()));

		// Test that we can modify the inner EventType through deref_mut
		*event = EventType::Unknown("modified".to_string());

		match &*event {
			EventType::Unknown(message) => {
				assert_eq!(message, "modified");
			}
			_ => panic!("Expected Unknown event type"),
		}
	}
}
