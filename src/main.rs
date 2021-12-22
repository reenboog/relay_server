use std::{
	fs,
	io::{Read, Write},
	net::{TcpListener, TcpStream},
	thread,
	time::Duration, sync::{mpsc, Arc, Mutex},
};

struct Worker {
	id: usize,
	thread: Option<thread::JoinHandle<()>>
}

impl Worker {
	fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Message>>>) -> Self {
		Self {
			id,
			thread: Some(thread::spawn(move || loop {
				// Acquiring a lock might fail if the mutex is in a poisoned state, which can happen if some 
				// other thread panicked while holding the lock rather than releasing the lock. In this situation, 
				// calling unwrap to have this thread panic is the correct action to take

				// The call to recv blocks, so if there is no job yet, the current thread will wait until a 
				// job becomes available. The Mutex<T> ensures that only one Worker thread at a time is trying to request a job.

				// This is a scope-based mutex that locks for as long as the variable lives
				match receiver.lock().unwrap().recv().unwrap() {
					Message::NewJob(job) => {
						println!("executing job {}", id);

						job();
					},
					Message::Terminate => {
						println!("Received Terminate for {}", id);

						break;
					}
				}
			}))
		}
	}
}

struct ThreadPool {
	workers: Vec<Worker>,
	sender: mpsc::Sender<Message>
}

type Job = Box<dyn FnOnce() + Send + 'static>;

enum Message {
	NewJob(Job),
	Terminate
}

impl Drop for ThreadPool {
	fn drop(&mut self) {
		println!("Sending Terminate to all workers");

		// we need two loops to avoid deadlocks
		// If we used a single loop to iterate through each worker, on the first iteration a terminate 
		// message would be sent down the channel and join called on the first worker’s thread. If that 
		// first worker was busy processing a request at that moment, the second worker would pick up the 
		// terminate message from the channel and shut down. We would be left waiting on the first worker 
		// to shut down, but it never would because the second thread picked up the terminate message. Deadlock!
		for _ in &self.workers {
			self.sender.send(Message::Terminate).unwrap();
		}

		for worker in &mut self.workers {
			println!("Shutting down worker {}", worker.id);

			if let Some(thread) = worker.thread.take() {
				thread.join().unwrap();
			}
		}
	}
}

impl ThreadPool {
	/// Creates a new ThreadPool.
	///
	/// The size is the number of threads in the pool.
	///
	/// # Panics
	///
	/// The `new` function will panic if the size is zero.
	pub fn new(size: usize) -> Self {
		assert!(size > 0);

		let mut workers = Vec::with_capacity(size);
		let (tx, rx) = mpsc::channel();

		let receiver = Arc::new(Mutex::new(rx));

		for i in 0..size {
			workers.push(Worker::new(i, Arc::clone(&receiver)));
		}

		Self {
			workers,
			sender: tx 
		}
	}

	pub fn execute<F>(&self, f: F) where F: FnOnce() + Send + 'static {
		let job = Box::new(f);

		self.sender.send(Message::NewJob(job)).unwrap();
	}
}

fn main() {
	// 1 thread pool: idle threads waiting
	// 2 incoming requests queue: when a thread is available, pop

	// connecting to a port is caleld binding
	let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
	let pool = ThreadPool::new(4);

	for stream in listener.incoming() {
		let mut stream = stream.unwrap();

		pool.execute(move || {
			handle_connection(&mut stream);
		});
	}
}

fn handle_connection(stream: &mut TcpStream) {
	let mut buffer = [0; 1024];

	// `read` keeps track of what data it returns internally; hence, we need mut
	stream.read(&mut buffer).unwrap();

	let get = b"GET / HTTP/1.1\r\n";
	let sleep = b"GET /sleep HTTP/1.1\r\n";

	let (status, file_name) = if buffer.starts_with(get) {
		("HTTP/1.1 200 OK", "index.html")
	} else if buffer.starts_with(sleep) {
		thread::sleep(Duration::from_secs(5));

		("HTTP/1.1 200 OK", "index.html")
	} else {
		("HTTP/1.1 404 NOT FOUND", "404.html")
	};

	let contents = fs::read_to_string(file_name).unwrap();
	let response = format!(
		"{}\r\nContent-Length: {}\r\n\r\n{}",
		status,
		contents.len(),
		contents
	);

	stream.write(response.as_bytes()).unwrap();
	stream.flush().unwrap();
}
