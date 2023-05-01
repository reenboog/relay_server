use database::Database;
use shared::{serde_json, Message};
use tokio::time::{self, Instant};

use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use tokio::time::Duration;

mod clients;
mod database;

#[derive(Clone, Debug)]
struct MsgToSend(Vec<u8>);
type Clients = Arc<Mutex<HashMap<String, mpsc::UnboundedSender<Message>>>>;
type DB = Arc<Mutex<Database>>;

use tokio::sync::mpsc;

async fn handle_client(clients: Clients, db: DB, stream: TcpStream) -> Result<(), Box<dyn Error>> {
	let mut client_id = String::new();
	let (tx, mut rx) = mpsc::unbounded_channel();
	let (mut stream_read, mut stream_write) = stream.into_split();

	let writer_task = tokio::spawn(async move {
		while let Some(msg) = rx.recv().await {
			let serialized_msg = serde_json::to_vec(&msg).unwrap();
			if let Err(e) = stream_write.write_all(&serialized_msg).await {
				eprintln!("Error writing to the stream: {:?}", e);
			}
		}
	});

	let mut ping_interval = tokio::time::interval(Duration::from_secs(6));
	let mut last_ping_received = Instant::now();

	loop {
		let mut buffer = [0; 1024];

		tokio::select! {
			_ = ping_interval.tick() => {
				if last_ping_received.elapsed() >= Duration::from_secs(6) {
					eprintln!("Client {} did not send a Ping within 5 seconds, disconnecting.", client_id);

					break;
				}
			}
			n = stream_read.read(&mut buffer) => {
				let n = match n {
					Ok(n) => n,
					Err(e) => {
						eprintln!("Error reading from the stream: {:?}", e);
						break;
					}
				};

				if n == 0 {
					break;
				}

				let message: Message = serde_json::from_slice(&buffer[..n])?;
				println!("{:?}", message);

				match message {
						Message::Hello { id } => {
							client_id = id;
							clients.lock().await.insert(client_id.clone(), tx.clone());

							println!("all clients: {:?}", clients.lock().await.keys());

							// TODO: DRY
							// Send all stored messages to the client if any
							if let Some(messages) = db.lock().await.get(client_id.clone()).await {
								for (_, message) in messages {
										if let Err(e) = tx.send(message.clone()) {
												eprintln!(
														"Error sending stored message to the writer task: {:?}",
														e
												);
										}
								}
							}
						}
						Message::Ping { .. } => {
								last_ping_received = Instant::now();
								let pong_msg = Message::Pong;
								if let Err(e) = tx.send(pong_msg) {
										eprintln!("Error sending Pong message to the writer task: {:?}", e);
								}
						}
						Message::Msg { sender, ts, payload, receiver } => {
							// here, store the message in a db and send ServerAck
							let msg = Message::Msg {
								sender,
								ts,
								payload,
								receiver: receiver.clone(),
							};

							db.lock().await.save(&msg).await?;

							let ack_msg = Message::ServerAck { id: ts };

							if let Err(e) = tx.send(ack_msg) {
								eprintln!("Error sending ServerAck message to the writer task: {:?}", e);
							}

							// FIXME: send this message instead?
							if let Some(receiver_tx) = clients.lock().await.get(&receiver) {
								// send all messages
								if let Some(msgs) = db.lock().await.get(client_id.clone()).await {
									for (_, msg) in msgs {
										if let Err(e) = receiver_tx.send(msg.clone()) {
												eprintln!("Error sending relayed message to the writer task: {:?}", e);
										}
									}
								}
							}
						}
						Message::ClientAck { id } => {
							db.lock().await.remove(client_id.clone(), id).await;
						}
					_ => (),
				}
			}
		}
	}

	clients.lock().await.remove(&client_id);
	drop(tx); // Close the channel to let the writer_task terminate
	writer_task.await?; // Wait for the writer_task to finish

	// Send a Left message to all other clients
	for (cid, client_tx) in clients.lock().await.iter() {
		if client_id != *cid {
			if let Err(e) = client_tx.send(Message::Left {
				id: client_id.clone(),
			}) {
				eprintln!("Error sending Left message to the writer task: {:?}", e);
			}
		}
	}

	Ok(())
}

pub async fn run_server(clients: Clients, addr: &str) -> Result<(), Box<dyn Error>> {
	let listener = TcpListener::bind(addr).await?;
	let db = Arc::new(Mutex::new(database::Database::new()));

	println!("Server is listening on {}", addr);

	loop {
		let (stream, _) = listener.accept().await?;
		let clients = clients.clone();
		let db = db.clone();

		tokio::spawn(async move {
			if let Err(e) = handle_client(clients, db, stream).await {
				eprintln!("Error handling client: {:?}", e);
			}
		});
	}
}

pub async fn run_status_watchdog(clients: Clients) -> Result<(), Box<dyn Error>> {
	loop {
		for (_, client_tx) in clients.lock().await.iter() {
			if let Err(e) = client_tx.send(Message::Status {
				ctx: "up".to_string(),
			}) {
				eprintln!("Error sending Left message to the writer task: {:?}", e);
			}
		}

		time::sleep(Duration::from_secs(10)).await;
	}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let addr = "127.0.0.1:8080";
	let clients: Clients = Arc::new(Mutex::new(HashMap::new()));

	let cs = clients.clone();
	let server = tokio::spawn(async move {
		run_server(cs, addr).await.unwrap();
	});

	let status_watchdog = tokio::spawn(async move {
		run_status_watchdog(clients).await.unwrap();
	});

	_ = tokio::join!(server, status_watchdog);

	Ok(())
}
