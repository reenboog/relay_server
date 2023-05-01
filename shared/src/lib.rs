pub use serde::{Deserialize, Serialize};
pub use serde_json;

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
	Hello { id: String },
	Msg { payload: String },
}
