use shared::Message;

use rand::Rng;
use shared::serde_json;

use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn generate_client_id() -> String {
	let mut rng = rand::thread_rng();
	let c: char = rng.gen_range('a'..='z');
	let n: u32 = rng.gen_range(0..10);
	format!("{}{}", c, n)
}

use tokio::io::{AsyncBufReadExt, BufReader};

async fn run_client(addr: &str, client_id: &str) -> Result<(), Box<dyn Error>> {
	let mut stream = TcpStream::connect(addr).await?;
	println!("Connected to the server");

	let hello_msg = Message::Hello {
		id: client_id.to_string(),
	};
	let serialized_hello = serde_json::to_string(&hello_msg)?;
	stream.write_all(serialized_hello.as_bytes()).await?;

	let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
	let mut buffer = [0; 1024];

	let mut stdin = BufReader::new(tokio::io::stdin());
	let mut std_in_buf = String::new();

	loop {
		tokio::select! {
			_ = interval.tick() => {
				let ping_msg = Message::Ping { id: client_id.to_string(), ctx: None };
				let serialized_ping = serde_json::to_string(&ping_msg)?;
				stream.write_all(serialized_ping.as_bytes()).await?
			}
			result = stream.read(&mut buffer) => {
				let n = match result {
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
				match message {
					Message::Msg { payload, receiver } => {
						if receiver == client_id {
							println!("Received message: {:?}", payload);
						}
					}
					Message::Left { id } => {
						println!("{} left", id);
					}
					Message::Pong => {
						println!("\rReceived Pong from the server");
					}
					_ => (),
				}
			}
			line = stdin.read_line(&mut std_in_buf) => {
				let line = match line {
					Ok(line) => line,
					Err(e) => {
						eprintln!("Error reading from stdin: {:?}", e);
						break;
					}
				};

				if line == 0 {
					break;
				}

				let msg = if std_in_buf.to_string() == String::from("qq\n") {
					Message::Quit { id: client_id.to_string() }
				} else {

					let line = std_in_buf.splitn(2, " ").map(|s| s.to_string()).collect::<Vec<String>>();
					let receiver = line[0].clone();
					let payload = line[1].clone();

					Message::Msg { payload, receiver }
				};

				stream.write_all(serde_json::to_string(&msg)?.as_bytes()).await?;

				std_in_buf.clear();
			}
		}
	}

	Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let addr = "127.0.0.1:8080";

	let client = tokio::spawn(async move {
		run_client(addr, &generate_client_id()).await.unwrap();
	});

	_ = tokio::join!(client);

	Ok(())
}
