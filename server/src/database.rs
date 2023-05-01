use std::collections::HashMap;

use shared::Message;
use tokio::sync::Mutex;

pub struct Database {
	messages: HashMap<String, HashMap<u64, Message>>,
}

impl Database {
	pub fn new() -> Self {
		Self {
			messages: HashMap::new(),
		}
	}

	pub async fn save(&mut self, message: &Message) -> Result<(), String> {
		match message {
			Message::Msg { ts, receiver, .. } => {
				self.messages
					.entry(receiver.clone())
					.or_insert_with(HashMap::new)
					.insert(*ts, message.clone());
			}
			_ => {}
		}

		Ok(())
	}

	pub async fn get(&self, receiver_id: String) -> Option<&HashMap<u64, Message>> {
		self.messages.get(&receiver_id)
	}

	pub async fn remove(&mut self, receiver_id: String, msg_ts: u64) {
		if let Some(msgs) = self.messages.get_mut(&receiver_id) {
			msgs.remove(&msg_ts);
		}
	}
}
