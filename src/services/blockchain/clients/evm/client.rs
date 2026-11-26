//! EVM-compatible blockchain client implementation.
//!
//! This module provides functionality to interact with Ethereum and other EVM-compatible
//! blockchains, supporting operations like block retrieval, transaction receipt lookup,
//! and log filtering.

use std::{
	collections::{BTreeSet, HashMap},
	marker::PhantomData,
	sync::{
		atomic::{AtomicU64, Ordering},
		Arc, Mutex,
	},
};

use anyhow::Context;
use async_trait::async_trait;
use futures::{stream, StreamExt, TryStreamExt};
use serde_json::json;
use tokio::sync::OnceCell;
use tracing::instrument;

use crate::{
	models::{BlockType, EVMBlock, EVMReceiptLog, EVMTransactionReceipt, Network},
	services::{
		blockchain::{
			client::{BlockChainClient, BlockFetchResult, FetchStreamKind},
			transports::{BlockchainTransport, EVMTransportClient},
			BlockFilterFactory,
		},
		filter::{evm_helpers::string_to_h256, EVMBlockFilter},
	},
};

/// Logs of one `eth_getLogs` range request, grouped by block number. `None` when the range
/// request failed and its blocks must be fetched one by one.
type RangeLogs = Option<Arc<HashMap<u64, Vec<EVMReceiptLog>>>>;

/// One range request held by [`LogsRangeCache`].
struct CachedLogsRange {
	from_block: u64,
	to_block: u64,
	addresses: Option<Vec<String>>,
	/// Insertion order, for eviction
	inserted: u64,
	logs: Arc<OnceCell<RangeLogs>>,
}

/// Number of range requests kept in memory. A batch is filtered at most 32 blocks at a time,
/// so only a few ranges are in use at once; the least recently inserted are dropped.
const MAX_CACHED_LOG_RANGES: usize = 64;

#[derive(Default)]
struct CachedLogsRanges {
	ranges: Vec<CachedLogsRange>,
	inserted: u64,
}

impl CachedLogsRanges {
	fn insert(
		&mut self,
		from_block: u64,
		to_block: u64,
		addresses: &Option<Vec<String>>,
		logs: Arc<OnceCell<RangeLogs>>,
	) {
		self.inserted += 1;
		self.ranges.push(CachedLogsRange {
			from_block,
			to_block,
			addresses: addresses.clone(),
			inserted: self.inserted,
			logs,
		});
		while self.ranges.len() > MAX_CACHED_LOG_RANGES {
			if let Some(oldest) = (0..self.ranges.len()).min_by_key(|&i| self.ranges[i].inserted) {
				self.ranges.swap_remove(oldest);
			}
		}
	}
}

/// Serves single-block `eth_getLogs` calls from one request per range of blocks.
///
/// The filter asks for the logs of one block at a time. With a range configured, a block not
/// covered yet triggers one request for up to `range` blocks, and the other blocks of it are
/// answered from that request. Concurrent callers for blocks of the same range share the
/// in-flight request.
///
/// A new range starts at the block that needs it, so blocks already processed are never
/// requested again, and stops before any cached range of the same filter above it. It never
/// reaches past the highest block this client has fetched with
/// `get_blocks`: the block watcher only fetches confirmed blocks, so a range never covers a
/// block that is not confirmed yet.
struct LogsRangeCache {
	range: u64,
	highest_fetched_block: AtomicU64,
	ranges: Mutex<CachedLogsRanges>,
}

impl LogsRangeCache {
	fn new(range: u64) -> Self {
		Self {
			range,
			highest_fetched_block: AtomicU64::new(0),
			ranges: Mutex::new(CachedLogsRanges::default()),
		}
	}

	fn note_fetched_block(&self, block_number: u64) {
		self.highest_fetched_block
			.fetch_max(block_number, Ordering::AcqRel);
	}

	/// The range covering `block_number` for `addresses`, creating it when no cached range
	/// covers the block, or `None` when the block must be fetched alone: it was not fetched
	/// yet, or a new range would hold only this block.
	fn range_for(
		&self,
		block_number: u64,
		addresses: &Option<Vec<String>>,
	) -> Option<(u64, u64, Arc<OnceCell<RangeLogs>>)> {
		let head = self.highest_fetched_block.load(Ordering::Acquire);
		if block_number > head {
			return None;
		}
		let mut cached = self.ranges.lock().unwrap_or_else(|e| e.into_inner());
		let same_filter = |range: &&CachedLogsRange| range.addresses == *addresses;
		if let Some(range) = cached
			.ranges
			.iter()
			.filter(same_filter)
			.find(|range| (range.from_block..=range.to_block).contains(&block_number))
		{
			return Some((range.from_block, range.to_block, range.logs.clone()));
		}

		// The range starts at the block itself, so it never covers blocks already processed,
		// and stops before a cached range above it, which a block processed out of order can
		// leave
		let next_cached = cached
			.ranges
			.iter()
			.filter(same_filter)
			.map(|range| range.from_block)
			.filter(|from| *from > block_number)
			.min()
			.unwrap_or(u64::MAX);
		let to_block = block_number
			.saturating_add(self.range - 1)
			.min(head)
			.min(next_cached - 1);
		if block_number >= to_block {
			return None;
		}
		let logs = Arc::new(OnceCell::new());
		cached.insert(block_number, to_block, addresses, logs.clone());
		Some((block_number, to_block, logs))
	}

	/// Stores logs fetched elsewhere for `[from_block, to_block]`, so the blocks of that range
	/// are answered from them.
	fn prime(
		&self,
		from_block: u64,
		to_block: u64,
		addresses: &Option<Vec<String>>,
		logs: Arc<HashMap<u64, Vec<EVMReceiptLog>>>,
	) {
		let mut cached = self.ranges.lock().unwrap_or_else(|e| e.into_inner());
		cached.insert(
			from_block,
			to_block,
			addresses,
			Arc::new(OnceCell::new_with(Some(Some(logs)))),
		);
	}
}

/// Client implementation for Ethereum Virtual Machine (EVM) compatible blockchains
///
/// Provides high-level access to EVM blockchain data and operations through HTTP transport.
#[derive(Clone)]
pub struct EvmClient<T: Send + Sync + Clone> {
	/// The underlying HTTP transport client for RPC communication
	http_client: T,
	/// Set when the network has a `logs_block_range` greater than 1
	logs_cache: Option<Arc<LogsRangeCache>>,
	/// Set by [`Self::with_logs_first`]: the `eth_getLogs` address filter that decides which
	/// blocks `get_blocks_with_meta` fetches
	logs_first_addresses: Option<Vec<String>>,
}

impl<T: Send + Sync + Clone> EvmClient<T> {
	/// Creates a new EVM client instance with a specific transport client
	pub fn new_with_transport(http_client: T) -> Self {
		Self {
			http_client,
			logs_cache: None,
			logs_first_addresses: None,
		}
	}

	/// Makes `get_blocks_with_meta` fetch only the blocks holding a log emitted by one of
	/// `addresses`, found with `eth_getLogs` over the requested range, instead of every
	/// block of the range.
	///
	/// Only correct when no monitor of the network can match a transaction without one of
	/// those logs; see `logs_first_address_filter`. `addresses` must be the same filter the
	/// block filter sends, so that its per-block `eth_getLogs` calls are answered from the
	/// logs fetched here when a `logs_block_range` is set.
	pub fn with_logs_first(mut self, addresses: Vec<String>) -> Self {
		self.logs_first_addresses = Some(addresses);
		self
	}

	/// Fetches the logs of up to `range` consecutive blocks with one `eth_getLogs` request.
	/// See `Network::logs_block_range`. `None` or 1 keeps one request per block.
	pub fn with_logs_block_range(mut self, range: Option<u64>) -> Self {
		self.logs_cache = range
			.filter(|range| *range > 1)
			.map(|range| Arc::new(LogsRangeCache::new(range)));
		self
	}
}

impl EvmClient<EVMTransportClient> {
	/// Creates a new EVM client instance
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC endpoints and chain details
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - New client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, anyhow::Error> {
		let client = EVMTransportClient::new(network).await?;
		Ok(Self::new_with_transport(client).with_logs_block_range(network.logs_block_range))
	}
}

impl<T: Send + Sync + Clone + BlockchainTransport> BlockFilterFactory<Self> for EvmClient<T> {
	type Filter = EVMBlockFilter<Self>;
	fn filter() -> Self::Filter {
		EVMBlockFilter {
			_client: PhantomData,
		}
	}
}

/// Extended functionality specific to EVM-compatible blockchains
#[async_trait]
pub trait EvmClientTrait {
	/// Retrieves a transaction receipt by its hash
	///
	/// # Arguments
	/// * `transaction_hash` - The hash of the transaction to look up
	///
	/// # Returns
	/// * `Result<TransactionReceipt, anyhow::Error>` - Transaction receipt or error
	async fn get_transaction_receipt(
		&self,
		transaction_hash: String,
	) -> Result<EVMTransactionReceipt, anyhow::Error>;

	/// Retrieves logs for a range of blocks
	///
	/// # Arguments
	/// * `from_block` - Starting block number
	/// * `to_block` - Ending block number
	/// * `addresses` - Optional list of addresses to filter logs by
	/// # Returns
	/// * `Result<Vec<Log>, anyhow::Error>` - Collection of matching logs or error
	async fn get_logs_for_blocks(
		&self,
		from_block: u64,
		to_block: u64,
		addresses: Option<Vec<String>>,
	) -> Result<Vec<EVMReceiptLog>, anyhow::Error>;
}

#[async_trait]
impl<T: Send + Sync + Clone + BlockchainTransport> EvmClientTrait for EvmClient<T> {
	/// Retrieves a transaction receipt by hash with proper error handling
	#[instrument(skip(self), fields(transaction_hash))]
	async fn get_transaction_receipt(
		&self,
		transaction_hash: String,
	) -> Result<EVMTransactionReceipt, anyhow::Error> {
		let hash = string_to_h256(&transaction_hash)
			.map_err(|e| anyhow::anyhow!("Invalid transaction hash: {}", e))?;

		let params = json!([format!("0x{:x}", hash)])
			.as_array()
			.with_context(|| "Failed to create JSON-RPC params array")?
			.to_vec();

		let response = self
			.http_client
			.send_raw_request(
				"eth_getTransactionReceipt",
				Some(serde_json::Value::Array(params)),
			)
			.await
			.with_context(|| format!("Failed to get transaction receipt: {}", transaction_hash))?;

		// Extract the "result" field from the JSON-RPC response
		let receipt_data = response
			.get("result")
			.with_context(|| "Missing 'result' field")?;

		// Handle null response case
		if receipt_data.is_null() {
			return Err(anyhow::anyhow!("Transaction receipt not found"));
		}

		Ok(serde_json::from_value(receipt_data.clone())
			.with_context(|| "Failed to parse transaction receipt")?)
	}

	/// Retrieves logs within the specified block range
	///
	/// # Arguments
	/// * `from_block` - Starting block number
	/// * `to_block` - Ending block number
	/// * `addresses` - Optional list of addresses to filter logs by
	/// # Returns
	/// * `Result<Vec<EVMReceiptLog>, anyhow::Error>` - Collection of matching logs or error
	#[instrument(skip(self, addresses))]
	async fn get_logs_for_blocks(
		&self,
		from_block: u64,
		to_block: u64,
		addresses: Option<Vec<String>>,
	) -> Result<Vec<EVMReceiptLog>, anyhow::Error> {
		if from_block == to_block {
			if let Some(logs) = self.block_logs_from_range(from_block, &addresses).await {
				return Ok(logs);
			}
		}
		self.request_logs(from_block, to_block, addresses).await
	}
}

impl<T: Send + Sync + Clone + BlockchainTransport> EvmClient<T> {
	/// The logs of `block_number` taken from its range request, or `None` when the block must
	/// be fetched on its own: no range is configured, the request has no address filter (a
	/// range of every log of a busy chain can exceed provider limits), the block is not
	/// covered by a range yet, or the range request failed.
	async fn block_logs_from_range(
		&self,
		block_number: u64,
		addresses: &Option<Vec<String>>,
	) -> Option<Vec<EVMReceiptLog>> {
		let cache = self.logs_cache.as_ref()?;
		addresses.as_ref()?;
		let (from_block, to_block, cell) = cache.range_for(block_number, addresses)?;
		let by_block = cell
			.get_or_init(|| async {
				match self
					.request_logs(from_block, to_block, addresses.clone())
					.await
				{
					Ok(logs) => group_logs_by_block(logs, from_block, to_block),
					Err(error) => {
						tracing::warn!(
							from_block,
							to_block,
							error = %error,
							"eth_getLogs range request failed; fetching its blocks one by one"
						);
						None
					}
				}
			})
			.await
			.as_ref()?;
		Some(by_block.get(&block_number).cloned().unwrap_or_default())
	}

	/// Sends one `eth_getLogs` request for `[from_block, to_block]`.
	async fn request_logs(
		&self,
		from_block: u64,
		to_block: u64,
		addresses: Option<Vec<String>>,
	) -> Result<Vec<EVMReceiptLog>, anyhow::Error> {
		// Convert parameters to JSON-RPC format
		let params = json!([{
			"fromBlock": format!("0x{:x}", from_block),
			"toBlock": format!("0x{:x}", to_block),
			"address": addresses
		}])
		.as_array()
		.with_context(|| "Failed to create JSON-RPC params array")?
		.to_vec();

		let response = self
			.http_client
			.send_raw_request("eth_getLogs", Some(params))
			.await
			.with_context(|| {
				format!(
					"Failed to get logs for blocks: {} - {}",
					from_block, to_block
				)
			})?;

		// Extract the "result" field from the JSON-RPC response
		let logs_data = response
			.get("result")
			.with_context(|| "Missing 'result' field")?;

		// Parse the response into the expected type
		serde_json::from_value(logs_data.clone()).with_context(|| "Failed to parse logs")
	}
}

/// Groups a range response by block. `None` (fetch the blocks one by one) if a log has no
/// block number or one outside the requested range, since it cannot be assigned to a block.
fn group_logs_by_block(logs: Vec<EVMReceiptLog>, from_block: u64, to_block: u64) -> RangeLogs {
	let mut by_block: HashMap<u64, Vec<EVMReceiptLog>> = HashMap::new();
	for log in logs {
		match log.block_number.map(|n| n.to::<u64>()) {
			Some(n) if (from_block..=to_block).contains(&n) => {
				by_block.entry(n).or_default().push(log)
			}
			block_number => {
				tracing::warn!(
					from_block,
					to_block,
					?block_number,
					"eth_getLogs range returned a log outside the range; fetching its blocks one by one"
				);
				return None;
			}
		}
	}
	Some(Arc::new(by_block))
}

#[async_trait]
impl<T: Send + Sync + Clone + BlockchainTransport> BlockChainClient for EvmClient<T> {
	/// Retrieves the latest block number with retry functionality
	#[instrument(skip(self))]
	async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error> {
		let response = self
			.http_client
			.send_raw_request::<serde_json::Value>("eth_blockNumber", None)
			.await
			.with_context(|| "Failed to get latest block number")?;

		// Extract the "result" field from the JSON-RPC response
		let hex_str = response
			.get("result")
			.and_then(|v| v.as_str())
			.ok_or_else(|| anyhow::anyhow!("Missing 'result' field"))?;

		// Parse hex string to u64
		u64::from_str_radix(hex_str.trim_start_matches("0x"), 16)
			.map_err(|e| anyhow::anyhow!("Failed to parse block number: {}", e))
	}

	/// Retrieves blocks within the specified range with retry functionality
	///
	/// # Note
	/// If end_block is None, only the start_block will be retrieved
	#[instrument(skip(self), fields(start_block, end_block))]
	async fn get_blocks(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<Vec<BlockType>, anyhow::Error> {
		let blocks = stream::iter(start_block..=end_block.unwrap_or(start_block))
			.map(|block_number| self.fetch_block(block_number))
			.buffered(32)
			.try_collect::<Vec<BlockType>>()
			.await?;

		if let Some(cache) = &self.logs_cache {
			if let Some(highest) = blocks.iter().filter_map(|block| block.number()).max() {
				cache.note_fetched_block(highest);
			}
		}
		Ok(blocks)
	}

	/// With [`EvmClient::with_logs_first`], fetches only the blocks of the range that hold a
	/// log of a monitored address, as a sparse stream. Otherwise every block, as a dense one.
	#[instrument(skip(self), fields(start_block, end_block))]
	async fn get_blocks_with_meta(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<BlockFetchResult, anyhow::Error> {
		let Some(addresses) = &self.logs_first_addresses else {
			return Ok(BlockFetchResult {
				blocks: self.get_blocks(start_block, end_block).await?,
				failed_blocks: Vec::new(),
				stream_kind: FetchStreamKind::Dense,
			});
		};
		let end_block = end_block.unwrap_or(start_block);

		// The watcher asks for confirmed blocks only, so the whole requested range may be
		// covered by log ranges, including blocks that turn out to have no logs
		if let Some(cache) = &self.logs_cache {
			cache.note_fetched_block(end_block);
		}

		let range = self.logs_cache.as_ref().map_or(1, |cache| cache.range);
		let addresses = Some(addresses.clone());
		let mut block_numbers = BTreeSet::new();
		let mut from_block = start_block;
		while from_block <= end_block {
			let to_block = std::cmp::min(from_block.saturating_add(range - 1), end_block);
			let by_block = match self
				.request_logs(from_block, to_block, addresses.clone())
				.await
			{
				Ok(logs) => group_logs_by_block(logs, from_block, to_block),
				Err(error) => {
					tracing::warn!(
						from_block,
						to_block,
						error = %error,
						"eth_getLogs request failed; fetching every block of the range"
					);
					None
				}
			};
			match by_block {
				Some(by_block) => {
					block_numbers.extend(by_block.keys().copied());
					// The block filter asks for the same logs again, block by block
					if let Some(cache) = &self.logs_cache {
						cache.prime(from_block, to_block, &addresses, by_block);
					}
				}
				// Cannot tell which blocks have logs: fetch them all, as a dense fetch would
				None => block_numbers.extend(from_block..=to_block),
			}
			from_block = to_block.saturating_add(1);
		}

		// A block that fails to fetch fails the batch, as in a dense fetch, so the checkpoint
		// stays before it and the next run fetches it again
		let blocks = stream::iter(block_numbers)
			.map(|block_number| self.fetch_block(block_number))
			.buffered(32)
			.try_collect::<Vec<BlockType>>()
			.await?;
		Ok(BlockFetchResult {
			blocks,
			failed_blocks: Vec::new(),
			stream_kind: FetchStreamKind::Sparse,
		})
	}
}

impl<T: Send + Sync + Clone + BlockchainTransport> EvmClient<T> {
	/// Fetches one block with its full transactions.
	async fn fetch_block(&self, block_number: u64) -> Result<BlockType, anyhow::Error> {
		let params = json!([
			format!("0x{:x}", block_number),
			true // include full transaction objects
		]);

		// A clone per block, as the concurrent fetches always used
		let client = self.http_client.clone();
		let response = client
			.send_raw_request("eth_getBlockByNumber", Some(params))
			.await
			.with_context(|| format!("Failed to get block: {}", block_number))?;

		let block_data = response
			.get("result")
			.ok_or_else(|| anyhow::anyhow!("Missing 'result' field"))?;

		if block_data.is_null() {
			return Err(anyhow::anyhow!("Block not found"));
		}

		let block: EVMBlock = serde_json::from_value(block_data.clone())
			.map_err(|e| anyhow::anyhow!("Failed to parse block: {}", e))?;

		Ok(BlockType::EVM(Box::new(block)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::services::blockchain::transports::TransportError;
	use reqwest_middleware::ClientWithMiddleware;
	use serde::Serialize;
	use serde_json::Value;
	use tracing_test::traced_test;

	/// Transport whose requests always fail after logging, as the endpoint manager does
	/// when an RPC answers HTTP 429
	#[derive(Clone)]
	struct FailingTransport;

	#[async_trait]
	impl BlockchainTransport for FailingTransport {
		async fn get_current_url(&self) -> String {
			"http://localhost:8545".to_string()
		}

		async fn send_raw_request<P>(
			&self,
			method: &str,
			_params: Option<P>,
		) -> Result<Value, TransportError>
		where
			P: Into<Value> + Send + Clone + Serialize,
		{
			tracing::warn!(
				"Request {} failed with status 429 Too Many Requests",
				method
			);
			Err(TransportError::network("rate limit exceeded", None, None))
		}

		fn update_endpoint_manager_client(
			&mut self,
			_client: ClientWithMiddleware,
		) -> Result<(), anyhow::Error> {
			Ok(())
		}
	}

	#[tokio::test]
	#[traced_test]
	async fn test_get_logs_for_blocks_span_records_block_range() {
		let client = EvmClient::new_with_transport(FailingTransport);

		let addresses = Some(vec![
			"0xdac17f958d2ee523a2206206994597c13d831ec7".to_string()
		]);
		let result = client
			.get_logs_for_blocks(11931475, 11931476, addresses)
			.await;

		assert!(result.is_err());
		// Errors logged inside the span name the blocks whose logs could not be fetched
		assert!(logs_contain("from_block=11931475"));
		assert!(logs_contain("to_block=11931476"));
		// The address list can be every monitored address, so it stays out of the span
		assert!(!logs_contain("0xdac17f958d2ee523a2206206994597c13d831ec7"));
	}

	/// Transport that answers `eth_getLogs` with one log per block of the requested range
	/// (only for `blocks_with_logs` when set) and `eth_getBlockByNumber` with an empty block,
	/// and records every request
	#[derive(Clone, Default)]
	struct RecordingTransport {
		requests: Arc<Mutex<Vec<(u64, u64)>>>,
		block_requests: Arc<Mutex<Vec<u64>>>,
		fail_ranges: bool,
		blocks_with_logs: Option<Vec<u64>>,
		failing_blocks: Vec<u64>,
	}

	impl RecordingTransport {
		fn requests(&self) -> Vec<(u64, u64)> {
			let mut requests = self.requests.lock().unwrap().clone();
			requests.sort();
			requests
		}

		fn block_requests(&self) -> Vec<u64> {
			let mut requests = self.block_requests.lock().unwrap().clone();
			requests.sort();
			requests
		}
	}

	fn parse_block(value: &Value) -> u64 {
		u64::from_str_radix(value.as_str().unwrap().trim_start_matches("0x"), 16).unwrap()
	}

	#[async_trait]
	impl BlockchainTransport for RecordingTransport {
		async fn get_current_url(&self) -> String {
			"http://localhost:8545".to_string()
		}

		async fn send_raw_request<P>(
			&self,
			method: &str,
			params: Option<P>,
		) -> Result<Value, TransportError>
		where
			P: Into<Value> + Send + Clone + Serialize,
		{
			let params: Value = params.unwrap().into();
			if method == "eth_getBlockByNumber" {
				let block_number = parse_block(&params[0]);
				self.block_requests.lock().unwrap().push(block_number);
				if self.failing_blocks.contains(&block_number) {
					return Err(TransportError::network("timeout", None, None));
				}
				let mut block = EVMBlock::default();
				block.0.number = Some(alloy::primitives::U64::from(block_number));
				return Ok(json!({ "result": serde_json::to_value(block).unwrap() }));
			}
			assert_eq!(method, "eth_getLogs");
			let from_block = parse_block(&params[0]["fromBlock"]);
			let to_block = parse_block(&params[0]["toBlock"]);
			self.requests.lock().unwrap().push((from_block, to_block));
			if self.fail_ranges && from_block != to_block {
				return Err(TransportError::network("block range too large", None, None));
			}
			let logs: Vec<Value> = (from_block..=to_block)
				.filter(|block| {
					self.blocks_with_logs
						.as_ref()
						.is_none_or(|blocks| blocks.contains(block))
				})
				.map(|block| {
					json!({
						"address": "0xdac17f958d2ee523a2206206994597c13d831ec7",
						"topics": [],
						"data": "0x",
						"blockHash": null,
						"blockNumber": format!("0x{:x}", block),
						"transactionHash": null,
						"transactionIndex": null,
						"logIndex": null,
						"transactionLogIndex": null,
						"logType": null,
						"removed": null
					})
				})
				.collect();
			Ok(json!({ "result": logs }))
		}

		fn update_endpoint_manager_client(
			&mut self,
			_client: ClientWithMiddleware,
		) -> Result<(), anyhow::Error> {
			Ok(())
		}
	}

	fn monitored() -> Option<Vec<String>> {
		Some(vec![
			"0xdac17f958d2ee523a2206206994597c13d831ec7".to_string()
		])
	}

	fn client_with_range(
		transport: &RecordingTransport,
		range: u64,
		highest_fetched_block: u64,
	) -> EvmClient<RecordingTransport> {
		let client =
			EvmClient::new_with_transport(transport.clone()).with_logs_block_range(Some(range));
		client
			.logs_cache
			.as_ref()
			.unwrap()
			.note_fetched_block(highest_fetched_block);
		client
	}

	fn log_blocks(logs: &[EVMReceiptLog]) -> Vec<u64> {
		logs.iter()
			.map(|log| log.block_number.unwrap().to::<u64>())
			.collect()
	}

	#[tokio::test]
	async fn test_logs_range_serves_concurrent_blocks_from_one_request() {
		let transport = RecordingTransport::default();
		let client = client_with_range(&transport, 10, 109);

		let results = futures::future::join_all(
			(100..=109).map(|block| client.get_logs_for_blocks(block, block, monitored())),
		)
		.await;

		for (block, result) in (100..=109).zip(results) {
			assert_eq!(log_blocks(&result.unwrap()), vec![block]);
		}
		assert_eq!(transport.requests(), vec![(100, 109)]);
	}

	#[tokio::test]
	async fn test_logs_range_never_reaches_past_the_highest_fetched_block() {
		let transport = RecordingTransport::default();
		let client = client_with_range(&transport, 10, 104);

		let logs = client
			.get_logs_for_blocks(102, 102, monitored())
			.await
			.unwrap();
		assert_eq!(log_blocks(&logs), vec![102]);
		// Not fetched yet: requested on its own, never as part of a range
		let logs = client
			.get_logs_for_blocks(105, 105, monitored())
			.await
			.unwrap();
		assert_eq!(log_blocks(&logs), vec![105]);

		assert_eq!(transport.requests(), vec![(102, 104), (105, 105)]);
	}

	#[tokio::test]
	async fn test_logs_range_extends_once_more_blocks_are_fetched() {
		let transport = RecordingTransport::default();
		let client = client_with_range(&transport, 10, 104);

		for block in [103, 104] {
			client
				.get_logs_for_blocks(block, block, monitored())
				.await
				.unwrap();
		}
		client.logs_cache.as_ref().unwrap().note_fetched_block(109);
		for block in 105..=109 {
			let logs = client
				.get_logs_for_blocks(block, block, monitored())
				.await
				.unwrap();
			assert_eq!(log_blocks(&logs), vec![block]);
		}

		// Each range starts at the block that needs it: no block is requested twice
		assert_eq!(transport.requests(), vec![(103, 104), (105, 109)]);
	}

	#[tokio::test]
	async fn test_logs_range_never_reaches_below_the_block() {
		let transport = RecordingTransport::default();
		let client = client_with_range(&transport, 100_000_000, 21_000_123);

		client
			.get_logs_for_blocks(21_000_100, 21_000_100, monitored())
			.await
			.unwrap();

		assert_eq!(transport.requests(), vec![(21_000_100, 21_000_123)]);
	}

	#[tokio::test]
	async fn test_logs_range_does_not_refetch_as_the_head_advances_block_by_block() {
		let transport = RecordingTransport::default();
		let client = client_with_range(&transport, 1000, 100);
		let cache = client.logs_cache.clone().unwrap();

		for block in 100..=110 {
			cache.note_fetched_block(block + 1);
			let logs = client
				.get_logs_for_blocks(block, block, monitored())
				.await
				.unwrap();
			assert_eq!(log_blocks(&logs), vec![block]);
		}

		let requests = transport.requests();
		// Each block is requested exactly once across all ranges
		let requested_blocks: u64 = requests.iter().map(|(from, to)| to - from + 1).sum();
		assert_eq!(requests.first(), Some(&(100, 101)));
		assert_eq!(requested_blocks, 12);
	}

	#[tokio::test]
	async fn test_logs_range_does_not_refetch_with_the_head_at_the_block() {
		let transport = RecordingTransport::default();
		let client = client_with_range(&transport, 1000, 100);
		let cache = client.logs_cache.clone().unwrap();

		for block in 100..=110 {
			cache.note_fetched_block(block);
			let logs = client
				.get_logs_for_blocks(block, block, monitored())
				.await
				.unwrap();
			assert_eq!(log_blocks(&logs), vec![block]);
		}

		// Each block is the head when filtered, so it is requested alone, and never again
		let expected: Vec<(u64, u64)> = (100..=110).map(|block| (block, block)).collect();
		assert_eq!(transport.requests(), expected);
	}

	#[tokio::test]
	async fn test_logs_range_does_not_overlap_a_range_above_it() {
		let transport = RecordingTransport::default();
		let client = client_with_range(&transport, 10, 104);
		client
			.get_logs_for_blocks(102, 102, monitored())
			.await
			.unwrap();

		// The head advances and a later block is processed before 105
		client.logs_cache.as_ref().unwrap().note_fetched_block(119);
		client
			.get_logs_for_blocks(112, 112, monitored())
			.await
			.unwrap();
		let logs = client
			.get_logs_for_blocks(105, 105, monitored())
			.await
			.unwrap();

		assert_eq!(log_blocks(&logs), vec![105]);
		// 105 fills the gap up to the range above it instead of overlapping it
		assert_eq!(
			transport.requests(),
			vec![(102, 104), (105, 111), (112, 119)]
		);
	}

	#[tokio::test]
	async fn test_logs_range_evicts_the_oldest_inserted_range() {
		let transport = RecordingTransport::default();
		let client = client_with_range(&transport, 2, 10_000);

		// Fill the cache with ranges above block 1000, then ask for an older one
		for block in (2000..2000 + 2 * MAX_CACHED_LOG_RANGES as u64).step_by(2) {
			client
				.get_logs_for_blocks(block, block, monitored())
				.await
				.unwrap();
		}
		client
			.get_logs_for_blocks(1000, 1000, monitored())
			.await
			.unwrap();
		// The older range just inserted is kept, so its other block needs no request
		client
			.get_logs_for_blocks(1001, 1001, monitored())
			.await
			.unwrap();

		let requests = transport.requests();
		assert_eq!(requests.len(), MAX_CACHED_LOG_RANGES + 1);
		assert_eq!(requests[0], (1000, 1001));
		// The first range inserted was the one evicted
		client
			.get_logs_for_blocks(2000, 2000, monitored())
			.await
			.unwrap();
		assert_eq!(transport.requests().len(), MAX_CACHED_LOG_RANGES + 2);
	}

	#[tokio::test]
	#[traced_test]
	async fn test_logs_range_failure_falls_back_to_single_blocks() {
		let transport = RecordingTransport {
			fail_ranges: true,
			..Default::default()
		};
		let client = client_with_range(&transport, 10, 109);

		for block in 100..=102 {
			let logs = client
				.get_logs_for_blocks(block, block, monitored())
				.await
				.unwrap();
			assert_eq!(log_blocks(&logs), vec![block]);
		}

		// The failed range is tried once; its blocks then go one by one
		assert_eq!(
			transport.requests(),
			vec![(100, 100), (100, 109), (101, 101), (102, 102)]
		);
		assert!(logs_contain("eth_getLogs range request failed"));
	}

	#[tokio::test]
	async fn test_logs_range_needs_an_address_filter() {
		let transport = RecordingTransport::default();
		let client = client_with_range(&transport, 10, 109);

		client.get_logs_for_blocks(100, 100, None).await.unwrap();
		client.get_logs_for_blocks(101, 101, None).await.unwrap();

		assert_eq!(transport.requests(), vec![(100, 100), (101, 101)]);
	}

	#[tokio::test]
	async fn test_without_logs_range_each_block_has_its_own_request() {
		let transport = RecordingTransport::default();
		let client =
			EvmClient::new_with_transport(transport.clone()).with_logs_block_range(Some(1));
		assert!(client.logs_cache.is_none());

		client
			.get_logs_for_blocks(100, 100, monitored())
			.await
			.unwrap();
		client
			.get_logs_for_blocks(101, 101, monitored())
			.await
			.unwrap();
		// Explicit multi-block requests are never served from the cache
		client
			.get_logs_for_blocks(100, 101, monitored())
			.await
			.unwrap();

		assert_eq!(
			transport.requests(),
			vec![(100, 100), (100, 101), (101, 101)]
		);
	}

	#[tokio::test]
	async fn test_get_blocks_extends_the_logs_ranges() {
		let transport = RecordingTransport::default();
		let client =
			EvmClient::new_with_transport(transport.clone()).with_logs_block_range(Some(10));

		// Nothing fetched yet: no range can be built
		client
			.get_logs_for_blocks(100, 100, monitored())
			.await
			.unwrap();
		client.get_blocks(100, Some(104)).await.unwrap();
		client
			.get_logs_for_blocks(101, 101, monitored())
			.await
			.unwrap();

		assert_eq!(transport.requests(), vec![(100, 100), (101, 104)]);
	}

	#[tokio::test]
	async fn test_logs_range_of_a_single_block_is_requested_alone() {
		let transport = RecordingTransport::default();
		let client = client_with_range(&transport, 10, 104);
		client
			.get_logs_for_blocks(101, 101, monitored())
			.await
			.unwrap();

		// 105 is the head and right after the cached range: its range would hold only itself
		client.logs_cache.as_ref().unwrap().note_fetched_block(105);
		client
			.get_logs_for_blocks(105, 105, monitored())
			.await
			.unwrap();

		assert_eq!(transport.requests(), vec![(101, 104), (105, 105)]);
	}

	#[test]
	#[traced_test]
	fn test_group_logs_by_block_rejects_logs_outside_the_range() {
		let log = |block: Option<u64>| EVMReceiptLog {
			address: Default::default(),
			topics: vec![],
			data: vec![].into(),
			block_hash: None,
			block_number: block.map(alloy::primitives::U64::from),
			transaction_hash: None,
			transaction_index: None,
			log_index: None,
			transaction_log_index: None,
			log_type: None,
			removed: None,
		};

		let grouped = group_logs_by_block(
			vec![log(Some(100)), log(Some(100)), log(Some(102))],
			100,
			104,
		)
		.unwrap();
		assert_eq!(grouped.get(&100).map(Vec::len), Some(2));
		assert_eq!(grouped.get(&102).map(Vec::len), Some(1));
		assert!(group_logs_by_block(vec![log(Some(105))], 100, 104).is_none());
		assert!(group_logs_by_block(vec![log(None)], 100, 104).is_none());
		assert!(logs_contain(
			"eth_getLogs range returned a log outside the range"
		));
	}

	fn block_numbers(result: &BlockFetchResult) -> Vec<u64> {
		let mut numbers: Vec<u64> = result
			.blocks
			.iter()
			.filter_map(|block| block.number())
			.collect();
		numbers.sort();
		numbers
	}

	fn logs_first_client(
		transport: &RecordingTransport,
		range: Option<u64>,
	) -> EvmClient<RecordingTransport> {
		EvmClient::new_with_transport(transport.clone())
			.with_logs_block_range(range)
			.with_logs_first(monitored().unwrap())
	}

	#[tokio::test]
	async fn test_logs_first_fetches_only_blocks_with_logs() {
		let transport = RecordingTransport {
			blocks_with_logs: Some(vec![103, 117]),
			..Default::default()
		};
		let client = logs_first_client(&transport, Some(10));

		let result = client.get_blocks_with_meta(100, Some(129)).await.unwrap();

		assert_eq!(result.stream_kind, FetchStreamKind::Sparse);
		assert_eq!(block_numbers(&result), vec![103, 117]);
		assert!(result.failed_blocks.is_empty());
		assert_eq!(transport.block_requests(), vec![103, 117]);
		assert_eq!(
			transport.requests(),
			vec![(100, 109), (110, 119), (120, 129)]
		);

		// The block filter's per-block calls are answered from the same requests
		for block in [103, 117] {
			let logs = client
				.get_logs_for_blocks(block, block, monitored())
				.await
				.unwrap();
			assert_eq!(log_blocks(&logs), vec![block]);
		}
		assert_eq!(transport.requests().len(), 3);
	}

	#[tokio::test]
	async fn test_logs_first_scans_only_the_requested_blocks() {
		let transport = RecordingTransport {
			blocks_with_logs: Some(vec![103, 106, 111, 116]),
			..Default::default()
		};
		let client = logs_first_client(&transport, Some(10));

		let result = client.get_blocks_with_meta(105, Some(116)).await.unwrap();

		assert_eq!(block_numbers(&result), vec![106, 111, 116]);
		// Ranges start at the first requested block: blocks already processed are not scanned
		assert_eq!(transport.requests(), vec![(105, 114), (115, 116)]);

		// The next batch continues where this one ended, and the block filter's calls for
		// this batch need no request
		for block in [106, 111, 116] {
			client
				.get_logs_for_blocks(block, block, monitored())
				.await
				.unwrap();
		}
		client.get_blocks_with_meta(117, Some(120)).await.unwrap();
		assert_eq!(
			transport.requests(),
			vec![(105, 114), (115, 116), (117, 120)]
		);
	}

	#[tokio::test]
	async fn test_logs_first_without_range_asks_each_block() {
		let transport = RecordingTransport {
			blocks_with_logs: Some(vec![101]),
			..Default::default()
		};
		let client = logs_first_client(&transport, None);

		let result = client.get_blocks_with_meta(100, Some(102)).await.unwrap();

		assert_eq!(block_numbers(&result), vec![101]);
		assert_eq!(
			transport.requests(),
			vec![(100, 100), (101, 101), (102, 102)]
		);
	}

	#[tokio::test]
	#[traced_test]
	async fn test_logs_first_failed_range_fetches_every_block_of_it() {
		let transport = RecordingTransport {
			fail_ranges: true,
			blocks_with_logs: Some(vec![]),
			..Default::default()
		};
		let client = logs_first_client(&transport, Some(10));

		let result = client.get_blocks_with_meta(100, Some(104)).await.unwrap();

		assert_eq!(block_numbers(&result), vec![100, 101, 102, 103, 104]);
		assert_eq!(result.stream_kind, FetchStreamKind::Sparse);
		assert!(logs_contain("fetching every block of the range"));
	}

	#[tokio::test]
	async fn test_logs_first_fails_the_batch_when_a_block_with_logs_fails_to_fetch() {
		let transport = RecordingTransport {
			blocks_with_logs: Some(vec![101, 102]),
			failing_blocks: vec![102],
			..Default::default()
		};
		let client = logs_first_client(&transport, Some(10));

		// The scan succeeds but a block with logs cannot be fetched: the batch fails, as a
		// dense fetch does, so the watcher keeps its checkpoint before the block
		let result = client.get_blocks_with_meta(100, Some(109)).await;

		assert!(result.is_err());
		assert_eq!(transport.requests(), vec![(100, 109)]);
	}

	#[tokio::test]
	async fn test_without_logs_first_every_block_is_fetched() {
		let transport = RecordingTransport {
			blocks_with_logs: Some(vec![101]),
			..Default::default()
		};
		let client = EvmClient::new_with_transport(transport.clone());

		let result = client.get_blocks_with_meta(100, Some(102)).await.unwrap();

		assert_eq!(result.stream_kind, FetchStreamKind::Dense);
		assert_eq!(block_numbers(&result), vec![100, 101, 102]);
		assert!(transport.requests().is_empty());
	}
}
