use shared::{serde_json, Message};
use tokio::time::Instant;

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

use tokio::sync::mpsc;

async fn handle_client(clients: Clients, stream: TcpStream) -> Result<(), Box<dyn Error>> {
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

	let mut interval = tokio::time::interval(Duration::from_secs(6));
	let mut last_ping_received = Instant::now();

	loop {
		let mut buffer = [0; 1024];

		tokio::select! {
			_ = interval.tick() => {
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
						}
						Message::Ping { .. } => {
								last_ping_received = Instant::now();
								let pong_msg = Message::Pong;
								if let Err(e) = tx.send(pong_msg) {
										eprintln!("Error sending Pong message to the writer task: {:?}", e);
								}
						}
						Message::Msg { sender, ts, payload, receiver } => {
								if let Some(receiver_tx) = clients.lock().await.get(&receiver) {
										let msg = Message::Msg {
											sender,
											ts,
											payload,
											receiver: receiver.clone(),
										};
										if let Err(e) = receiver_tx.send(msg) {
												eprintln!("Error sending relayed message to the writer task: {:?}", e);
										}
								}
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
					if let Err(e) = client_tx.send(Message::Left { id: client_id.clone() }) {
							eprintln!("Error sending Left message to the writer task: {:?}", e);
					}
			}
	}

	Ok(())
}

pub async fn run_server(addr: &str) -> Result<(), Box<dyn Error>> {
	let listener = TcpListener::bind(addr).await?;

	println!("Server is listening on {}", addr);

	let clients: Clients = Arc::new(Mutex::new(HashMap::new()));

	loop {
		let (stream, _) = listener.accept().await?;
		let clients = clients.clone();
		tokio::spawn(async move {
			if let Err(e) = handle_client(clients, stream).await {
				eprintln!("Error handling client: {:?}", e);
			}
		});
	}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let addr = "127.0.0.1:8080";

	let server = tokio::spawn(async move {
		run_server(addr).await.unwrap();
	});

	_ = tokio::join!(server);

	Ok(())
}
