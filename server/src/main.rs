use shared::{serde_json, Frame, Message};

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};

use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use tokio::sync::Mutex;

type EventChannel = mpsc::Sender<Event>;
type Users = Arc<Mutex<HashMap<String, EventChannel>>>;
type Database = Arc<Mutex<HashMap<u64, Message>>>;

use tokio::sync::mpsc;

#[derive(Debug)]
enum Event {
	OnHello { user: String },
	// OnServerAck { ts: u64 }, TODO: is it actually required?
	OnAck { ts: u64 },
	OnMsgSent { msg: Message },
	OnMsgRcvd { msg: Message },
	OnMsgStored { msg: Message },
	OnMsgRemoved { ts: u64 },
}

// sent to the sender worker
// TODO: currently accepts frames, but should be stricter
#[derive(Debug)]
enum SendRequest {
	Send { msg: Frame },
}

// sent to the database worker
#[derive(Debug)]
enum DbRequest {
	Save { msg: Message },
	Remove { ts: u64 },
}

async fn handle_connection(stream: TcpStream, db: Database, users: Users) {
	let (read_stream, write_stream) = stream.into_split();
	let (event_tx, event_rx) = mpsc::channel(32);
	let (db_request_tx, db_request_rx) = mpsc::channel(32);
	let (sender_tx, sender_rx) = mpsc::channel(32);

	tokio::spawn(on_event(event_tx.clone(), event_rx, sender_tx, db_request_tx, users.clone()));
	tokio::spawn(on_db_write(db_request_rx, event_tx.clone(), db));
	tokio::spawn(on_send(sender_rx, write_stream));

	let mut buf_reader = BufReader::new(read_stream);

	loop {
		// TODO: make larger?
		let mut buf = [0; 1024];

		match buf_reader.read(&mut buf).await {
			Err(e) => {
				eprintln!("Error reading from the stream: {:?}", e);
				break;
			}
			Ok(0) => break,
			Ok(n) => {
				if let Ok(frame) = serde_json::from_slice(&buf[..n]) {
					match frame {
						Frame::Hello { user } => event_tx.send(Event::OnHello { user }).await,
						Frame::Msg { data: msg } => event_tx.send(Event::OnMsgSent { msg }).await,
						Frame::ClientAck { ts } => event_tx.send(Event::OnAck { ts }).await,
						_ => {
							eprintln!("Unexpected message type {:?} ", frame);
							Ok(())
						}
					}
					.unwrap() // TODO: don't hard unwrap
				} else {
					eprintln!("Error parsing from buffer: {:?}", buf);
					// TODO: should I break? -rather no
				}
			}
		}
	}
}

async fn on_db_write(
	mut db_rx: mpsc::Receiver<DbRequest>,
	event_tx: mpsc::Sender<Event>,
	db: Database,
) {
	while let Some(req) = db_rx.recv().await {
		match req {
			DbRequest::Save { msg } => {
				db.lock().await.insert(msg.ts, msg.clone());
				event_tx
					.send(Event::OnMsgStored { msg})
					.await
					.unwrap();
			}
			DbRequest::Remove { ts } => {
				db.lock().await.remove(&ts);
				event_tx.send(Event::OnMsgRemoved { ts }).await.unwrap();
			}
		}
	}
}

async fn on_send(mut send_rx: mpsc::Receiver<SendRequest>, mut write_stream: OwnedWriteHalf) {
	while let Some(req) = send_rx.recv().await {
		match req {
			SendRequest::Send { msg } => {
				if let Ok(serialized) = serde_json::to_vec(&msg) {
					if let Err(e) = write_stream.write_all(&serialized).await {
						eprintln!("Error writing to the stream: {:?}", e);
					}
				} else {
					eprintln!("Failed to serialize {:?}", msg);
				}
			}
		}
	}
}

async fn on_event(
	event_tx: mpsc::Sender<Event>,
	mut event_rx: mpsc::Receiver<Event>,
	sender_tx: mpsc::Sender<SendRequest>,
	db_request_tx: mpsc::Sender<DbRequest>,
	users: Users,
) {
	while let Some(event) = event_rx.recv().await {
		match event {
			Event::OnHello { user } => {
				let mut users = users.lock().await;
				users.insert(user.clone(), event_tx.clone());

				println!("Welcome, {} to the {:?}", user, users.keys());

				// TODO: send pending messages as wel
				// send next
			},
			Event::OnAck { ts } => {
				db_request_tx.send(DbRequest::Remove { ts }).await.unwrap();
			},
			Event::OnMsgSent { msg } => {
				println!("{} -> {}: {:?}", msg.sender, msg.receiver, msg.payload);

				db_request_tx.send(DbRequest::Save { msg }).await.unwrap();
			},
			Event::OnMsgRcvd { msg } => {
				sender_tx.send(SendRequest::Send { msg: Frame::Msg { data: msg } }).await.unwrap();
			},
			Event::OnMsgStored { msg } => {
				// TODO: run in parallel? join?
				sender_tx
					.send(SendRequest::Send {
						msg: Frame::ServerAck { ts: msg.ts },
					})
					.await
					.unwrap();

				if let Some(receiver_tx)= users.lock().await.get(&msg.receiver) {
					receiver_tx.send(Event::OnMsgRcvd { msg }).await.unwrap();
				}
			},
			Event::OnMsgRemoved { ts } => {
				// TODO: send missing, if any
			},
		}
	}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let addr = "127.0.0.1:8080";
	let listener = TcpListener::bind(addr).await.unwrap();
	let db: Database = Arc::new(Mutex::new(HashMap::new()));
	let users: Users = Arc::new(Mutex::new(HashMap::new()));

	loop {
		let (stream, _) = listener.accept().await.unwrap();
		let db = db.clone();
		let users = users.clone();

		tokio::spawn(async move {
			handle_connection(stream, db, users).await;
		});
	}
}
