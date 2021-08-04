use serde::Deserialize;

#[derive(Deserialize)]
pub struct HistoryEntry {
	pub changedToAt: Option<i128>,
	pub name: String
}

#[derive(Deserialize)]
pub struct MojangProfile {
	pub id: String
}

#[derive(Deserialize, Clone)]
pub struct MojangAuthenticationSelectedProfile {
	pub id: String
}

#[derive(Deserialize, Clone)]
pub struct MojangAuthenticationResponse {
	pub clientToken: String,
	pub accessToken: String,
	pub selectedProfile: MojangAuthenticationSelectedProfile
}

#[derive(Deserialize)]
pub struct MojangAnswer {
	pub id: u32
}

#[derive(Deserialize)]
pub struct MojangQuestionsResponseEntry {
	pub answer: MojangAnswer
}

#[derive(Deserialize)]
pub struct MojangTexture {
	pub timestamp: i128
}

#[derive(Deserialize, Debug)]
pub struct MojangProperty {
	pub name: String,
	pub value: String
}

#[derive(Deserialize, Debug)]
pub struct MojangSessionResponse {
	pub properties: Vec<MojangProperty>
}