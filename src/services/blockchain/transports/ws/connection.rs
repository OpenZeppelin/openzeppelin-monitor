use std::time::Instant;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

#[derive(Debug)]
pub struct WebSocketConnection {
	pub stream: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
	pub is_healthy: bool,
	last_activity: Instant,
}

impl WebSocketConnection {
	pub fn new() -> Self {
		Self {
			stream: None,
			is_healthy: false,
			last_activity: Instant::now(),
		}
	}

	pub fn is_connected(&self) -> bool {
		self.stream.is_some() && self.is_healthy
	}

	pub fn update_activity(&mut self) {
		self.last_activity = Instant::now();
	}
}
