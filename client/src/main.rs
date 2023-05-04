use shared::{Frame, Message};

use shared::serde_json;

use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

// fn generate_client_id() -> String {
// 	let mut rng = rand::thread_rng();
// 	let c: char = rng.gen_range('a'..='z');
// 	let n: u32 = rng.gen_range(0..10);
// 	format!("{}{}", c, n)
// }

use tokio::io::{AsyncBufReadExt, BufReader};

async fn run_client(addr: &str, client_id: &str) -> Result<(), Box<dyn Error>> {
	let mut stream = TcpStream::connect(addr).await?;
	println!("Connected to the server");

	let hello_msg = Frame::Hello {
		user: client_id.to_string(),
	};
	let serialized_hello = serde_json::to_vec(&hello_msg)?;
	stream.write_all(&[serialized_hello.as_slice(), b"\n"].concat()).await?;


	// let mut stdin = BufReader::new(tokio::io::stdin());
	// let mut std_in_buf = String::new();
	let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

	let users = vec!["aa", "bb", "cc", "dd"];

	let mut msg_counter = 0;
	let mut message_buffer = String::new();

	loop {
		let mut buffer = [0; 1024];

		tokio::select! {
			_ = interval.tick() => {
				use std::time::{SystemTime, UNIX_EPOCH};
				use rand::Rng;

				let ts = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time went backwards").as_millis() as u64;
				let receivers: Vec<String> = users.iter().filter(|u| **u != client_id.to_string()).map(|u| u.to_string()).collect();
				let receiver = receivers[rand::thread_rng().gen::<usize>() % receivers.len()].clone();
				let payload = format!("{} -> {}: {}", client_id, receiver, msg_counter);
				let msg = Frame::Msg { data: Message { ts, sender: client_id.to_string(), receiver, payload }};
				let serialized = serde_json::to_vec(&msg)?;

				msg_counter += 1;

				stream.write_all(&[serialized.as_slice(), b"\n"].concat()).await?
			}
			result = stream.read(&mut buffer) => {
				let n = match result {
					Ok(n) => {
						n
					},
					Err(e) => {
						eprintln!("Error reading from the stream: {:?}", e);
						break;
					}
				};

				if n == 0 {
					break;
				}

				message_buffer.push_str(&String::from_utf8_lossy(&buffer[..n]));

				while let Some(pos) = message_buffer.find('\n') {
					let message_str = message_buffer[..pos].trim().to_owned();
					message_buffer = message_buffer.split_off(pos + 1);

					if message_str.is_empty() {
							continue;
					}

					let message: Frame = match serde_json::from_str(&message_str) {
						Ok(message) => message,
						Err(e) => {
							eprintln!("Error parsing message: {:?}", e);
							continue;
						}
					};

					match message {
						Frame::Msg { data: Message { ts, sender, receiver, payload } } => {
							if receiver == client_id {
								println!("{}: {} {:?}", sender, ts, payload);
							}

							let ack = serde_json::to_vec(&Frame::ClientAck { ts })?;

							stream.write_all(&ack).await?;
						},
						_ => (),
					}
				}

				message_buffer.clear();
			}
		}
	}

	Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	use std::env;

	let addr = "0.0.0.0:8080";
	let args: Vec<String> = env::args().collect();


	// let mut stdin = BufReader::new(tokio::io::stdin());
	// let mut client_id = String::new();

	// stdin.read_line(&mut client_id).await;

	// client_id = client_id.replace("\n", "");
	let client_id = args[1].clone();

	println!("Hi, {}", client_id);

	_ = tokio::spawn(async move {
		match run_client(addr, &client_id).await {
			Ok(_) => todo!(),
			Err(e) => {
				eprintln!("Error running client: {}", e);
			}
		}
	})
	.await;

	// _ = tokio::join!(client);

	Ok(())
}
