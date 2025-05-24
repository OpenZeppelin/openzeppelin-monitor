//! Mock implementation of the Subxt client.
//! Copied from <https://github.com/paritytech/subxt/blob/master/core/src/events.rs>

/// Event related test utilities used outside this module.
pub(crate) mod subxt_utils {
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
		Test(Ev),
	}

	/// This encodes to the same format an event is expected to encode to
	/// in node System.Events storage.
	#[derive(Encode)]
	pub struct EventRecord<E: Encode> {
		phase: Phase,
		event: AllEvents<E>,
		topics: Vec<HashFor<SubstrateConfig>>,
	}

	impl<E: Encode> EventRecord<E> {
		/// Create a new event record with the given phase, event, and topics.
		pub fn new(phase: Phase, event: E, topics: Vec<HashFor<SubstrateConfig>>) -> Self {
			Self {
				phase,
				event: AllEvents::Test(event),
				topics,
			}
		}
	}

	/// Build an EventRecord, which encoded events in the format expected
	/// to be handed back from storage queries to System.Events.
	pub fn event_record<E: Encode>(phase: Phase, event: E) -> EventRecord<E> {
		EventRecord::new(phase, event, vec![])
	}

	/// Build fake metadata consisting of a single pallet that knows
	/// about the event type provided.
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

		subxt::Metadata::from(metadata)
	}

	/// Build an `Events` object for test purposes, based on the details provided,
	/// and with a default block hash.
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

	/// Much like [`events`], but takes pre-encoded events and event count, so that we can
	/// mess with the bytes in tests if we need to.
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

	/// Build an `Events` object for test purposes, based on the details provided,
	/// and with a default block hash.
	pub fn mock_empty_events() -> Events<SubstrateConfig> {
		let metadata = metadata::<()>();
		let event_records: Vec<EventRecord<()>> = vec![];
		events(metadata, event_records)
	}
}
