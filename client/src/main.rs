use shared::Message;

use std::io::{Read, Write};
use std::net::TcpStream;

use rand::{Rng};
use shared::serde_json;

fn generate_client_id() -> String {
	let mut rng = rand::thread_rng();
	let c: char = rng.gen_range('a'..='z');
	let n: u32 = rng.gen_range(0..10);
	format!("{}{}", c, n)
}

fn main() -> std::io::Result<()> {
	let mut stream = TcpStream::connect("127.0.0.1:8080")?;
	let client_id = generate_client_id();

	println!("Connected to server as {}", client_id);

	// Send Hello message to the server
	let hello_message = Message::Hello {
		id: client_id.clone(),
	};
	stream.write(serde_json::to_string(&hello_message).unwrap().as_bytes())?;

	loop {
		let mut payload = String::new();
		std::io::stdin().read_line(&mut payload)?;
		let message = Message::Msg { payload };

		stream.write(serde_json::to_string(&message).unwrap().as_bytes())?;

		let mut buf = [0; 1024];
		let n = stream.read(&mut buf)?;
		let response = String::from_utf8_lossy(&buf[..n]);
		println!("{}", response);
	}
}
