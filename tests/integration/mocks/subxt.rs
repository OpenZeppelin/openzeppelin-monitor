//! Mock implementation of the Subxt client for testing purposes.
//!
//! This module provides utilities for creating mock Subxt events and metadata for testing scenarios.
//! It's particularly useful for testing code that interacts with Substrate/Polkadot events without
//! requiring a live node connection.
//!
//! The implementation is based on the Subxt events implementation from Parity Technologies.
//! Original source: <https://github.com/paritytech/subxt/blob/master/core/src/events.rs>

use frame_metadata::{
	v15::{
		CustomMetadata, ExtrinsicMetadata, OuterEnums, PalletEventMetadata, PalletMetadata,
		RuntimeMetadataV15,
	},
	RuntimeMetadataPrefixed,
};
use parity_scale_codec::{Compact, Decode, Encode};
use scale_info::{meta_type, TypeInfo};
use subxt::config::{HashFor, SubstrateConfig};
use subxt::events::{Events, Phase};

/// An "outer" events enum containing exactly one event.
///
/// This enum is used to wrap individual events in a format that matches
/// the structure of events as they appear in the Substrate runtime.
#[derive(
	Encode,
	Decode,
	TypeInfo,
	Clone,
	Debug,
	PartialEq,
	Eq,
	scale_encode::EncodeAsType,
	scale_decode::DecodeAsType,
)]
pub enum AllEvents<Ev> {
	/// The wrapped event
	Test(Ev),
}

/// Represents an event record in the format used by Substrate's System.Events storage.
///
/// This struct encodes events in the same format that would be returned from
/// storage queries to System.Events, including phase information and optional topics.
#[derive(Encode)]
pub struct EventRecord<E: Encode> {
	/// The phase of the event (ApplyExtrinsic or Finalization)
	phase: Phase,
	/// The wrapped event
	event: AllEvents<E>,
	/// Optional topics associated with the event
	topics: Vec<HashFor<SubstrateConfig>>,
}

/// Creates fake metadata for testing purposes, containing a single pallet that knows about the provided event type.
///
/// # Arguments
///
/// * `E` - The event type to include in the metadata
pub fn metadata<E: TypeInfo + 'static>() -> subxt::Metadata {
	let pallets = vec![PalletMetadata {
		name: "Test",
		storage: None,
		calls: None,
		event: Some(PalletEventMetadata {
			ty: meta_type::<E>(),
		}),
		constants: vec![],
		error: None,
		index: 0,
		docs: vec![],
	}];

	let extrinsic = ExtrinsicMetadata {
		version: 0,
		signed_extensions: vec![],
		address_ty: meta_type::<()>(),
		call_ty: meta_type::<()>(),
		signature_ty: meta_type::<()>(),
		extra_ty: meta_type::<()>(),
	};

	let meta = RuntimeMetadataV15::new(
		pallets,
		extrinsic,
		meta_type::<()>(),
		vec![],
		OuterEnums {
			call_enum_ty: meta_type::<()>(),
			event_enum_ty: meta_type::<AllEvents<E>>(),
			error_enum_ty: meta_type::<()>(),
		},
		CustomMetadata {
			map: Default::default(),
		},
	);
	let runtime_metadata: RuntimeMetadataPrefixed = meta.into();
	let metadata: subxt::Metadata = runtime_metadata.try_into().unwrap();

	metadata
}

/// Creates an `Events` object for test purposes based on the provided metadata and event records.
///
/// # Arguments
///
/// * `metadata` - The metadata to use for decoding events
/// * `event_records` - The event records to include
pub fn events<E: Decode + Encode>(
	metadata: subxt::Metadata,
	event_records: Vec<EventRecord<E>>,
) -> Events<SubstrateConfig> {
	let num_events = event_records.len() as u32;
	let mut event_bytes = Vec::new();
	for ev in event_records {
		ev.encode_to(&mut event_bytes);
	}
	events_raw(metadata, event_bytes, num_events)
}

/// Creates an `Events` object from pre-encoded event bytes and event count.
///
/// This function is useful for testing scenarios where you need to work with
/// raw event bytes or manipulate the encoding.
///
/// # Arguments
///
/// * `metadata` - The metadata to use for decoding events
/// * `event_bytes` - The pre-encoded event bytes
/// * `num_events` - The number of events in the bytes
pub fn events_raw(
	metadata: subxt::Metadata,
	event_bytes: Vec<u8>,
	num_events: u32,
) -> Events<SubstrateConfig> {
	// Prepend compact encoded length to event bytes:
	let mut all_event_bytes = Compact(num_events).encode();
	all_event_bytes.extend(event_bytes);
	Events::decode_from(all_event_bytes, metadata)
}

/// Creates an empty `Events` object for test purposes.
///
/// This is useful for testing scenarios where you need to verify behavior
/// with no events present.
pub fn mock_empty_events() -> Events<SubstrateConfig> {
	let metadata = metadata::<()>();
	let event_records: Vec<EventRecord<()>> = vec![];
	events(metadata, event_records)
}
