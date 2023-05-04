use shared::{serde_json, Frame, Message};

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use tokio::sync::Mutex;

type EventChannel = mpsc::Sender<Event>;
type Users = Arc<Mutex<BTreeMap<String, EventChannel>>>;
type Database = Arc<Mutex<BTreeMap<u64, Message>>>;

use tokio::sync::mpsc;

#[derive(Debug)]
enum Event {
	OnHello { user: String },
	OnAck { ts: u64 },
	OnMsgSent { msg: Message },
	OnMsgRcvd { msg: Message },
	OnMsgStored { msg: Message },
	OnMsgRemoved { ts: u64 },
	OnFrameWritten { user: String },
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

	tokio::spawn(on_event(
		event_tx.clone(),
		event_rx,
		sender_tx,
		db_request_tx,
		users.clone(),
		db.clone(),
	));
	tokio::spawn(on_db_write(db_request_rx, event_tx.clone(), db));
	tokio::spawn(on_send(sender_rx, write_stream, event_tx.clone()));

	let mut buf_reader = BufReader::new(read_stream);

	let mut message_buffer = String::new();

	loop {
		let mut buf = [0; 1024];

		match buf_reader.read(&mut buf).await {
			Err(e) => {
				eprintln!("Error reading from the stream: {:?}", e);
				break;
			}
			Ok(0) => break,
			Ok(n) => {
				message_buffer.push_str(&String::from_utf8_lossy(&buf[..n]));

				while let Some(pos) = message_buffer.find('\n') {
					let message_str = message_buffer[..pos].trim().to_owned();
					message_buffer = message_buffer.split_off(pos + 1);

					if message_str.is_empty() {
							continue;
					}

					let frame: Frame = match serde_json::from_str(&message_str) {
						Ok(frame) => frame,
						Err(e) => {
							eprintln!("Error parsing message: {:?}", e);
							continue;
						}
					};

					match frame {
						Frame::Hello { user } => event_tx.send(Event::OnHello { user }).await,
						Frame::Msg { data: msg } => event_tx.send(Event::OnMsgSent { msg }).await,
						Frame::ClientAck { ts } => event_tx.send(Event::OnAck { ts }).await,
						_ => {
							eprintln!("Unexpected message type {:?} ", frame);
							Ok(())
						}
					}
					.unwrap();
				}
				
				message_buffer.clear();
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
				event_tx.send(Event::OnMsgStored { msg }).await.unwrap();
			}
			DbRequest::Remove { ts } => {
				db.lock().await.remove(&ts);
				event_tx.send(Event::OnMsgRemoved { ts }).await.unwrap();
			}
		}
	}
}

async fn on_send(
	mut send_rx: mpsc::Receiver<SendRequest>,
	mut write_stream: OwnedWriteHalf,
	event_tx: mpsc::Sender<Event>,
) {
	while let Some(req) = send_rx.recv().await {
		match req {
			SendRequest::Send { msg } => {
				if let Ok(serialized) = serde_json::to_vec(&msg) {
					// TODO: fix the delimiter issue
					if let Err(e) = write_stream
						.write_all(&[serialized.as_slice(), b"\n"].concat())
						.await
					{
						eprintln!("Error writing to the stream: {:?}", e);
					} else {
						if let Frame::Msg { data } = msg {
							event_tx
								.send(Event::OnFrameWritten {
									user: data.receiver,
								})
								.await
								.unwrap();
						}
					}
				} else {
					eprintln!("Failed to serialize {:?}", msg);
				}
			}
		}
	}
}

async fn send_next_for_user(
	user: String,
	db: Database,
	last_ts: &mut u64,
	sender_tx: mpsc::Sender<SendRequest>,
) {
	if let Some((ts, msg)) = db
		.lock()
		.await
		.iter()
		.filter(|(_, m)| m.receiver == user)
		.find(|(ts, _)| **ts > *last_ts)
	{
		*last_ts = *ts;

		sender_tx
			.send(SendRequest::Send {
				msg: Frame::Msg { data: msg.clone() },
			})
			.await
			.unwrap();
	}
}

async fn on_event(
	event_tx: mpsc::Sender<Event>,
	mut event_rx: mpsc::Receiver<Event>,
	sender_tx: mpsc::Sender<SendRequest>,
	db_request_tx: mpsc::Sender<DbRequest>,
	users: Users,
	db: Database,
) {
	let mut last_ts = 0;

	while let Some(event) = event_rx.recv().await {
		match event {
			Event::OnHello { user } => {
				let mut users = users.lock().await;
				users.insert(user.clone(), event_tx.clone());

				println!("Welcome, {} to the {:?}", user, users.keys());

				send_next_for_user(user, db.clone(), &mut last_ts, sender_tx.clone()).await;
			}
			Event::OnAck { ts } => {
				println!("received ack for {}", ts);

				db_request_tx.send(DbRequest::Remove { ts }).await.unwrap();
			}
			Event::OnMsgSent { msg } => {
				println!("{} -> {}: {:?}", msg.sender, msg.receiver, msg);

				db_request_tx.send(DbRequest::Save { msg }).await.unwrap();
			}
			Event::OnMsgRcvd { msg } => {
				send_next_for_user(msg.receiver, db.clone(), &mut last_ts, sender_tx.clone()).await;
			}
			Event::OnFrameWritten { user } => {
				send_next_for_user(user, db.clone(), &mut last_ts, sender_tx.clone()).await;
			}
			Event::OnMsgStored { msg } => {
				// TODO: run in parallel? join?
				sender_tx
					.send(SendRequest::Send {
						msg: Frame::ServerAck { ts: msg.ts },
					})
					.await
					.unwrap();

				if let Some(receiver_tx) = users.lock().await.get(&msg.receiver) {
					receiver_tx.send(Event::OnMsgRcvd { msg }).await.unwrap();
				}
			}
			Event::OnMsgRemoved { ts } => {
				// TODO: send missing, if any
			}
		}
	}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let addr = "0.0.0.0:8080";
	let listener = TcpListener::bind(addr).await.unwrap();
	let db: Database = Arc::new(Mutex::new(BTreeMap::new()));
	let users: Users = Arc::new(Mutex::new(BTreeMap::new()));

	loop {
		let (stream, _) = listener.accept().await.unwrap();
		let db = db.clone();
		let users = users.clone();

		tokio::spawn(async move {
			handle_connection(stream, db, users).await;
		});
	}
}
