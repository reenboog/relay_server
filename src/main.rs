use std::{net::{TcpListener, TcpStream}, io::{Read, Write}};
use std::fs;

fn main() {
    // connecting to a port is caleld binding
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();

    for stream in listener.incoming() {
        let mut stream = stream.unwrap();

        handle_connection(&mut stream);
    }
}

fn handle_connection(stream: &mut TcpStream) {
    let mut buffer = [0; 1024];

    // `read` keeps track of what data it returns internally; hence, we need mut
    stream.read(&mut buffer).unwrap();

    let get = b"GET / HTTP/1.1\r\r";

				let (status, file_name) = if buffer.starts_with(get) {
					("HTTP/1.1 200 OK", "index.html")
				} else {
					("HTTP/1.1 404 NOT FOUND", "404.html")
				};

				let contents = fs::read_to_string(file_name).unwrap();
				let response = format!("{}\r\nContent-Length: {}\r\n\r\n{}", status, contents.len(), contents);

				stream.write(response.as_bytes()).unwrap();
				stream.flush().unwrap();
}
