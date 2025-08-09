pub mod itch_types;

const ITCH_API_V1_BASE_URL: &str = "https://itch.io/api/1";
const ITCH_API_V2_BASE_URL: &str = "https://api.itch.io";

/// Verifies that a given Itch.io API key is valid
/// 
/// # Arguments
///
/// * `api_key` - The api_key to verify against the Itch.io servers
pub async fn verify_api_key(api_key: &str) -> Result<(), String> {
  let client: reqwest::Client = reqwest::Client::new();

  let response: itch_types::VerifyAPIKeyResponse = client.get(format!("{ITCH_API_V1_BASE_URL}/{api_key}/credentials/info"))
    .send()
    .await.map_err(|e| e.to_string())?
    .json()
    .await.map_err(|e| e.to_string())?;

  match response {
    itch_types::VerifyAPIKeyResponse::Success { .. } => Ok(()),
    itch_types::VerifyAPIKeyResponse::Error { errors } =>
      Err(format!(
        "Invalid api key: {}",
        errors.get(0).unwrap_or(&String::from(""))
      )),
  }
}

/// Gets the information about a game in Itch.io
/// 
/// # Arguments
///
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `game_id` - The ID of the game from which information will be obtained
pub async fn get_game_info(api_key: String, game_id: u64) -> Result<itch_types::Game, String> {
  
  let client: reqwest::Client = reqwest::Client::new();

  let response: itch_types::GameInfoResponse = client.get(format!("{ITCH_API_V2_BASE_URL}/games/{game_id}"))
    .header(reqwest::header::AUTHORIZATION, api_key)
    .header(reqwest::header::ACCEPT, "application/vnd.itch.v2")
    .send()
    .await.map_err(|e| e.to_string())?
    .json()
    .await.map_err(|e| e.to_string())?;

  match response {
    itch_types::GameInfoResponse::Success { game } => Ok(game),
    itch_types::GameInfoResponse::Error { errors } =>
      Err(format!(
        "The server replied with an error while trying to get the game info: {}",
        errors.get(0).unwrap_or(&String::from(""))
      ))
  }
}
