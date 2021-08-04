use crate::{constants, structs};

use std::time::{SystemTime, UNIX_EPOCH};
use serde_json::json;
use openssl::ssl::{SslConnector, SslMethod, SslStream};
use std::io::{Write, Read};
use std::net::{TcpStream};

pub struct Sniper {
	pub username: String,
	new_username: String,
	email: String,
	password: String,
	answers: Vec<String>,
	client: reqwest::Client,
	available_at: Option<i128>,
	pub auth: Option<structs::MojangAuthenticationResponse>
}

impl Sniper {
	pub fn new(
		username: String,
		new_username: String,
		email: String,
		password: String,
		answers: Vec<String>
	) -> Self {
		Self {
			username,
			new_username,
			email,
			password,
			answers,
			client: reqwest::Client::new(),
			available_at: None,
			auth: None
		}
	}

	// Converts a Minecraft username to a UUID
	async fn username_to_uuid(&self, username: &String) -> Result<String, reqwest::Error> {
		let response = self.client
			.get(format!("{}/users/profiles/minecraft/{}", constants::MOJANG_API_ROOT, username))
			.send()
			.await?
			.json::<structs::MojangProfile>()
			.await?;

		Ok(response.id)
	}

	// Fetches username change history of a UUID
	async fn uuid_to_name_history(&self, uuid: &String) -> Result<Vec<structs::HistoryEntry>, reqwest::Error> {
		let response = self.client
			.get(format!("{}/user/profiles/{}/names", constants::MOJANG_API_ROOT, uuid))
			.send()
			.await?
			.json::<Vec<structs::HistoryEntry>>()
			.await?;

		Ok(response)
	}

	// Gets the time that a username is available (37 days since change)
	pub async fn get_time_available_at(&mut self) -> Result<i128, reqwest::Error> {
		let uuid = self.username_to_uuid(&self.new_username).await?;
		let username_history = self.uuid_to_name_history(&uuid).await?;
		let mut index = username_history.len();

		for u in username_history.iter().rev() {
			if u.name.eq_ignore_ascii_case(&self.username) {
				break;
			}

			index -= 1;
		}

		let available_at = 3196800000 + username_history[index].changedToAt.unwrap_or(0);

		self.available_at = Some(available_at);

		Ok(available_at)
	}

	pub async fn authenticate(&mut self) -> Result<bool, reqwest::Error> {
		self.create_mojang_authtoken().await.unwrap();
		self.validate_mojang_authtoken().await
	}

	// Answers Mojang security questions
	async fn answer_security_questions(&self) -> Result<bool, reqwest::Error> {
		let auth = match &self.auth {
			Some(auth) => auth,
			None => {
				println!("answer_security_questions called before authenticating");

				std::process::exit(10);
			}
		};

		let questions = self.client
			.get(format!("{}/user/security/challenges", constants::MOJANG_API_ROOT))
			.header("Authorization", format!("Bearer {}", &auth.accessToken))
			.send()
			.await?
			.json::<Vec<structs::MojangQuestionsResponseEntry>>()
			.await?;

		let payload = json!([
			{
				"id": questions[0].answer.id,
				"answer": self.answers[0]
			},
			{
				"id": questions[1].answer.id,
				"answer": self.answers[1]
			},
			{
				"id": questions[2].answer.id,
				"answer": self.answers[2]
			}
		]);

		let status = self.client
			.post(format!("{}/user/security/location", constants::MOJANG_API_ROOT))
			.json(&payload)
			.header("Authorization", format!("Bearer {}", &auth.accessToken))
			.send()
			.await?
			.status();

		Ok(status == 204)
	}

	// Validates a Mojang authentication token
	async fn validate_mojang_authtoken(&self) -> Result<bool, reqwest::Error> {
		if !self.answers.is_empty() {
			self.answer_security_questions().await?;
		}

		let auth = match &self.auth {
			Some(auth) => auth,
			None => {
				println!("validate_mojang_authtoken called before authenticating");

				std::process::exit(10);
			}
		};

		let payload = json!({
			"accessToken": &auth.accessToken,
			"clientToken": &auth.clientToken
		});

		let status = self.client
			.post(format!("{}/validate", constants::YGGDRASIL_API_ROOT))
			.json(&payload)
			.header(reqwest::header::USER_AGENT, constants::USER_AGENT)
			.send()
			.await?
			.status();

		Ok(status == 204)
	}

	// Creates a Mojang authentication token from a username and password pair
	async fn create_mojang_authtoken(&mut self) -> Result<bool, reqwest::Error> {
		let payload = json!({
			"agent": {
				"name": "Minecraft",
				"version": 1
			},
			"username": self.email,
			"password": self.password,
			"requestUser": false
		});

		let response = self.client
			.post(format!("{}/authenticate", constants::YGGDRASIL_API_ROOT))
			.json(&payload)
			.header(reqwest::header::USER_AGENT, constants::USER_AGENT)
			.send()
			.await?
			.json::<structs::MojangAuthenticationResponse>()
			.await?;

		self.auth = Some(response);

		Ok(true)
	}

	pub async fn get_mojang_time_offset(&self) -> Result<i128, reqwest::Error> {
		let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i128;
		let data = self.client
			.get(format!("{}/session/minecraft/profile/c06f89064c8a49119c29ea1dbd1aab82", constants::MOJANG_SESSIONSERVER_API_ROOT))
			.send()
			.await?
			.json::<structs::MojangSessionResponse>()
			.await?;

		let mut json_raw: Option<&structs::MojangProperty> = None;

		for entry in data.properties.iter() {
			if entry.name.eq_ignore_ascii_case("textures") {
				json_raw = Some(entry);

				break;
			}
		}

		if let Some(entry) = json_raw {
			let decoded = base64::decode(&entry.value).unwrap();

			let json = match serde_json::from_str::<structs::MojangTexture>(std::str::from_utf8(&decoded).unwrap()) {
				Ok(data) => data,
				Err(reason) => {
					println!("could not deserialize json data from session: {}", reason);
		
					std::process::exit(8);
				}
			};
		
			println!("time difference: {}", json.timestamp - time);
		
			return Ok(json.timestamp - time);
		}

		println!("could not find texture data");

		std::process::exit(9);
	}
}

pub fn prepare_username_change(username: &String, accessToken: &String) -> Result<SslStream<TcpStream>, bool> {
	let connector = SslConnector::builder(SslMethod::tls()).unwrap().build();

	let stream = TcpStream::connect(format!("{}:443", constants::MINECRAFTSERVICES_API_ROOT)).unwrap();
	let mut stream = connector.connect(constants::MINECRAFTSERVICES_API_ROOT, stream).unwrap();

	stream
		.write_all(format!("PUT https://{}/minecraft/profile/name/{} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\n", constants::MINECRAFTSERVICES_API_ROOT, username, constants::MINECRAFTSERVICES_API_ROOT, accessToken).as_bytes())
		.unwrap();

	Ok(stream)
}

pub fn change_username_from_stream(stream: &mut SslStream<TcpStream>) -> () {
	stream.write(&vec![13, 10]).unwrap();

	let mut buffer = [0; 12];
	stream.read(&mut buffer).unwrap();

	let status = std::str::from_utf8(&buffer[9..])
		.unwrap().parse::<u16>()
		.unwrap();

	match status {
		200 => {
			println!("snipe successful");
		},
		_ => {
			println!("snipe failed");
		}
	}
}