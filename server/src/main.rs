use shared::{serde_json, Message};

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

fn handle_client(mut stream: TcpStream) {
	let mut buf = [0; 1024];

	// Receive Hello message from client and extract client ID
	let n = stream.read(&mut buf).unwrap();
	let hello_message = String::from_utf8_lossy(&buf[..n]);
	let client_id = match serde_json::from_str(&hello_message) {
		Ok(Message::Hello { id }) => id,
		_ => {
			println!("Received invalid message: {}", hello_message);
			return;
		}
	};
	println!("New connection: client_id={}", client_id);

	loop {
		match stream.read(&mut buf) {
			Ok(n) => {
				if n == 0 {
					println!("Connection closed: client_id={}", client_id);
					return;
				}
				let message = String::from_utf8_lossy(&buf[..n]);
				match serde_json::from_str(&message) {
					Ok(Message::Msg { payload }) => {
						println!(
							"Received message: client_id={}, payload={}",
							client_id, payload
						);
						let response = Message::Msg {
							payload: format!("-> {}: {}", client_id, payload),
						};
						stream
							.write(serde_json::to_string(&response).unwrap().as_bytes())
							.unwrap();
					}
					_ => {
						println!("Received invalid message: {}", message);
					}
				}
			}
			Err(e) => {
				eprintln!("Error reading from socket: {}", e);
				return;
			}
		}
	}
}

fn main() -> std::io::Result<()> {
	let listener = TcpListener::bind("127.0.0.1:8080")?;
	println!("Listening on {}", listener.local_addr()?);

	for stream in listener.incoming() {
		match stream {
			Ok(stream) => {
				std::thread::spawn(move || {
					handle_client(stream);
				});
			}
			Err(e) => {
				eprintln!("Error accepting connection: {}", e);
			}
		}
	}
	Ok(())
}
