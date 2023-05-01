pub use serde::{Deserialize, Serialize};
pub use serde_json;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[rustfmt::skip]
pub enum Message {
	Hello { id: String },
	Ping { id: String },
	Pong,
	Msg { sender: String, ts: u64, payload: String, receiver: String },
	Quit { id: String },
	Left { id: String },
	Status { ctx: String },
	ServerAck { id: u64 },
	ClientAck { id: u64 },
}
