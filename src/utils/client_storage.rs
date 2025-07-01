use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

/// Generic client storage that can hold any type of client (blockchain, transport, etc.)
///
/// Clients are stored in a thread-safe way using a HashMap and an RwLock.
/// The HashMap is indexed by the network slug and the value is an Arc of the client.
#[derive(Default)]
pub struct ClientStorage<T> {
	pub clients: Arc<RwLock<HashMap<String, Arc<T>>>>,
}

impl<T> ClientStorage<T> {
	pub fn new() -> Self {
		Self {
			clients: Arc::new(RwLock::new(HashMap::new())),
		}
	}
}
