use std::collections::HashMap;

use shared::Message;
use tokio::sync::Mutex;

struct Database {
	messages: Mutex<HashMap<String, HashMap<u64, Message>>>,
}

impl Database {
	fn new() -> Self {
		Self {
			messages: Mutex::new(HashMap::new()),
		}
	}

	async fn save(&self, message: &Message) -> Result<(), String> {
		let mut messages = self.messages.lock().await;
		match message {
			Message::Msg { ts, receiver, .. } => {
				messages
					.entry(receiver.clone())
					.or_insert_with(HashMap::new)
					.insert(*ts, message.clone());
			}
			_ => {}
		}
		Ok(())
	}
}
