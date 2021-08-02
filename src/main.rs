#![allow(non_snake_case)]

use std::{env, thread, cmp};
use std::time::{SystemTime, UNIX_EPOCH, Instant, Duration};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio;
use serde::Deserialize;
use serde_json::json;

static GLOBAL_THREAD_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Deserialize)]
struct HistoryEntry {
	changedToAt: Option<u128>
}

#[derive(Deserialize)]
struct MojangProfile {
	id: String
}

#[derive(Deserialize, Clone)]
struct MojangAuthenticationSelectedProfile {
	id: String
}

#[derive(Deserialize, Clone)]
struct MojangAuthenticationResponse {
	clientToken: String,
	accessToken: String,
	selectedProfile: MojangAuthenticationSelectedProfile
}

async fn username_to_uuid(username: &String, client: &reqwest::Client) -> Result<String, reqwest::Error> {
	let response = client
		.get(format!("https://api.mojang.com/users/profiles/minecraft/{}", username))
		.send()
		.await?
		.json::<MojangProfile>()
		.await?;

	Ok(response.id)
}

async fn uuid_to_name_history(uuid: &String, client: &reqwest::Client) -> Result<Vec<HistoryEntry>, reqwest::Error> {
	let response = client
		.get(format!("https://api.mojang.com/user/profiles/{}/names", uuid))
		.send()
		.await?
		.json::<Vec<HistoryEntry>>()
		.await?;

	Ok(response)
}

async fn get_time_available_at(username: &String, client: &reqwest::Client) -> Result<u128, reqwest::Error> {
	let uuid = username_to_uuid(username, &client).await?;
	let username_history = uuid_to_name_history(&uuid, &client).await?;

	let available_at = 3196800000 + match username_history.last() {
		Some(entry) => entry.changedToAt.unwrap_or(0),
		_ => 0
	};

	Ok(available_at)
}

async fn validate_mojang_authtoken(auth: &MojangAuthenticationResponse, client: &reqwest::Client) -> Result<bool, reqwest::Error> {
	let payload = json!({
		"accessToken": auth.accessToken,
		"clientToken": auth.clientToken
	});

	let status = client
		.post("https://authserver.mojang.com/validate")
		.json(&payload)
		.send()
		.await?
		.status();

	Ok(status == 204)
}

async fn create_mojang_authtoken(email: &String, password: &String, client: &reqwest::Client) -> Result<MojangAuthenticationResponse, reqwest::Error> {
	let payload = json!({
		"agent": {
			"name": "Minecraft",
			"version": 1
		},
		"username": email,
		"password": password,
		"requestUser": true
	});

	let response = client
		.post("https://authserver.mojang.com/authenticate")
		.json(&payload)
		.send()
		.await?
		.json::<MojangAuthenticationResponse>()
		.await?;

	Ok(response)
}

async fn calculate_ping(url: String, client: &reqwest::Client) -> Result<u128, reqwest::Error> {
	let time = Instant::now();

	client
		.head(url)
		.send()
		.await?;

	Ok(time.elapsed().as_millis())
}

fn change_username_sync(auth: &MojangAuthenticationResponse, desired_username: &String, password: &String, client: &reqwest::blocking::Client) -> Result<bool, reqwest::Error> {
	let payload = json!({
		"password": password,
		"name": desired_username
	});

	let status = client
		.post(format!("https://api.mojang.com/user/profile/{}/name", auth.selectedProfile.id))
		.json(&payload)
		.header("Authorization", format!("Bearer {}", auth.accessToken))
		.send()?
		.status();

	Ok(status == 200 || status == 204)
}

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
	let args: Vec<String> = env::args().collect();

	if args.len() != 5 {
		println!("Usage: {} <email> <password> <desired username> <current username of user that changed from desired username>", &args[0]);

		std::process::exit(5);
	}

	let client = reqwest::Client::new();

	let email = &args[1];
	let password = &args[2];
	let username = &args[3];
	let current_username = &args[4];

	let available_at = get_time_available_at(current_username, &client).await?;
	let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();

	let sleep_for = (cmp::max(available_at - now, 300000) - 300000) as u64;
	let sleep_duration = Duration::from_millis(sleep_for);

	println!("Username \"{}\" available in: {} milliseconds", username, available_at - now);
	println!("sleeping for: {} milliseconds", sleep_for);

	thread::sleep(sleep_duration);

	let auth = create_mojang_authtoken(email, password, &client).await?;
	
	println!("clientToken: {}", auth.clientToken);
	println!("accessToken: {}", auth.accessToken);

	let successful = validate_mojang_authtoken(&auth, &client).await?;

	if !successful {
		println!("validating authtoken failed, exiting...");

		std::process::exit(6);
	}

	let ping = match calculate_ping("https://authserver.mojang.com".to_string(), &client).await {
		Ok(ms) => ms,
		Err(reason) => {
			println!("{}", reason);

			std::process::exit(7);
		}
	};

	for i in 0..10 {
		GLOBAL_THREAD_COUNT.fetch_add(1, Ordering::SeqCst);

		let thread_i = i;
		let thread_username = username.clone();
		let thread_password = username.clone();
		let thread_auth = auth.clone();

		println!("spawning {}", i);

		thread::spawn(move || {
			let client = reqwest::blocking::Client::new();
			let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();

			thread::sleep(Duration::from_millis((available_at - now - ping) as u64));

			match change_username_sync(&thread_auth, &thread_username, &thread_password, &client) {
				Ok(name_changed) => println!("Name changed: {}", name_changed),
				_ => println!("error")
			}
	
			GLOBAL_THREAD_COUNT.fetch_sub(1, Ordering::SeqCst);

			println!("Thread #{} finished", thread_i);
		});
	}

	while GLOBAL_THREAD_COUNT.load(Ordering::SeqCst) != 0 {
		thread::sleep(Duration::from_millis(100)); 
	}

	Ok(())
}