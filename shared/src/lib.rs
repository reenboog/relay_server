pub use serde::{Deserialize, Serialize};
pub use serde_json;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
	pub ts: u64,
	pub sender: String,
	pub receiver: String,
	pub payload: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[rustfmt::skip]
pub enum Frame {
	Hello { user: String },
	// Ping { id: String },
	// Pong,
	Msg { data: Message },
	// Quit { id: String },
	// Left { id: String },
	// Status { ctx: String },
	// sent by the server
	ServerAck { ts: u64 },
	// sent by a client
	ClientAck { ts: u64 },
}
