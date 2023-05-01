use std::collections::HashMap;

use shared::Message;
use tokio::sync::{mpsc::Sender, Mutex};

struct Clients {
	inner: Mutex<HashMap<String, Sender<Message>>>,
}

impl Clients {
	fn new() -> Self {
		Self {
			inner: Mutex::new(HashMap::new()),
		}
	}

	async fn add(&self, id: String, tx: Sender<Message>) {
		self.inner.lock().await.insert(id, tx);
	}

	async fn remove(&self, id: &str) {
		self.inner.lock().await.remove(id);
	}

	async fn send_to(&self, id: &str, message: Message) -> Result<(), String> {
		let clients = self.inner.lock().await;
		match clients.get(id) {
			Some(sender) => {
				sender.send(message).await.map_err(|e| e.to_string())?;

				Ok(())
			}
			None => Err(format!("Client '{}' not found", id)),
		}
	}

	async fn broadcast(&self, message: Message) {
		let mut clients = self.inner.lock().await;
		for sender in clients.values_mut() {
			let _ = sender.send(message.clone()).await;
		}
	}

	// TODO: DRY
	async fn broadcast_except(&self, id: &str, message: Message) {
		let mut clients = self.inner.lock().await;
		for (client_id, sender) in clients.iter_mut() {
			if client_id != id {
				let _ = sender.send(message.clone()).await;
			}
		}
	}
}
