#![allow(non_snake_case)]

mod constants;
mod sniper;
mod structs;

use std::{env, thread, cmp};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use openssl::ssl::{SslStream};
use std::net::{TcpStream};

static GLOBAL_THREAD_COUNT: AtomicUsize = AtomicUsize::new(0);

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {	
	let args: Vec<String> = env::args().collect();

	let answers = match args.len() {
		c if c < 5 => {
			println!("Usage: {} <email> <password> <desired username> <current username of user that changed from desired username>", &args[0]);

			std::process::exit(5);
		},
		5 => Vec::new(),
		_ => {
			args[5]
				.split(";")
				.map(|x| x.to_string())
				.collect()
		}
	};

	let email = &args[1];
	let password = &args[2];
	let username = &args[3];
	let current_username = &args[4];

	let mut sniper = sniper::Sniper::new(
		username.to_string(),
		current_username.to_string(),
		email.to_string(),
		password.to_string(),
		answers
	);

	let available_at = sniper.get_time_available_at().await?;
	let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i128;

	let sleep_for = cmp::max(if available_at > now { available_at - now } else { 60000 }, 60000) as u64 - 60000;
	let sleep_duration = Duration::from_millis(sleep_for);

	println!("Username \"{}\" available in: {} milliseconds ({})", username, sleep_for, available_at);

	thread::sleep(sleep_duration);

	let successful = sniper.authenticate().await?;

	if !successful {
		println!("validating authtoken failed, exiting...");

		std::process::exit(6);
	}

	let ping = match sniper.get_mojang_time_offset().await {
		Ok(ms) => ms,
		Err(reason) => {
			println!("{}", reason);

			std::process::exit(8);
		}
	};

	for i in 0..5 {
		GLOBAL_THREAD_COUNT.fetch_add(1, Ordering::SeqCst);

		let thread_i = i;
		let thread_username = sniper.username.clone();
		let thread_auth = sniper.auth.clone().unwrap();
		
		thread::spawn(move || {
			let spin_sleeper = spin_sleep::SpinSleeper::new(100_000);
			let mut streams: Vec<SslStream<TcpStream>> = Vec::new();

			for _ in 0..1 {
				match sniper::prepare_username_change(&thread_username, &thread_auth.accessToken) {
					Ok(stream) => streams.push(stream),
					Err(_) => println!("error occured")
				}
			}

			let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i128;
			let sleep_duration = if available_at > now + ping { available_at - now - ping } else { 0 };

			spin_sleeper.sleep(Duration::from_millis(sleep_duration as u64));

			println!("Thread #{} started at {}", thread_i, SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis());

			for stream in streams.iter_mut() {
				// match change_username_sync(&thread_auth, &thread_username, &client) {
				// 	Ok(name_changed) => println!("Name changed: {}", name_changed),
				// 	_ => println!("error")
				// }

				sniper::change_username_from_stream(stream);
			}
	
			GLOBAL_THREAD_COUNT.fetch_sub(1, Ordering::SeqCst);

			println!("Thread #{} finished at {}", thread_i, SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis());
		});
	}

	while GLOBAL_THREAD_COUNT.load(Ordering::SeqCst) != 0 {
		thread::sleep(Duration::from_millis(100)); 
	}

	Ok(())
}