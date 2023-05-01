pub use serde::{Deserialize, Serialize};
pub use serde_json;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Message {
	Hello { id: String },
	Ping { id: String, ctx: Option<Vec<u8>> },
	Pong,
	Msg { payload: String, receiver: String },
	Quit { id: String },
	Left { id: String },
}
