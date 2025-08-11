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
        errors.join("\n")
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
pub async fn get_game_info(api_key: &str, game_id: u64) -> Result<itch_types::Game, String> {
  
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
        errors.join("\n")
      ))
  }
}

/// Gets the game's uploads (downloadable files)
/// 
/// # Arguments
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `game_id` - The ID of the game from which information will be obtained
pub async fn get_game_uploads(api_key: &str, game_id: u64) -> Result<Vec<itch_types::GameUpload>, String> {
    
  let client: reqwest::Client = reqwest::Client::new();

  let response: itch_types::GameUploadsResponse = client.get(format!("{ITCH_API_V2_BASE_URL}/games/{game_id}/uploads"))
    .header(reqwest::header::AUTHORIZATION, api_key)
    .header(reqwest::header::ACCEPT, "application/vnd.itch.v2")
    .send()
    .await.map_err(|e| e.to_string())?
    .json()
    .await.map_err(|e| e.to_string())?;

  match response {
    itch_types::GameUploadsResponse::Success { uploads } => Ok(uploads),
    itch_types::GameUploadsResponse::Error { errors } =>
      Err(format!(
        "The server replied with an error while trying to get the game uploads: {}",
        errors.join("\n")
      ))
  }
}

pub async fn get_collections(api_key: &str) -> Result<Vec<itch_types::Collection>, String> {

  let client: reqwest::Client = reqwest::Client::new();

  let response: itch_types::CollectionsResponse = client.get(format!("{ITCH_API_V2_BASE_URL}/profile/collections"))
    .header(reqwest::header::AUTHORIZATION, api_key)
    .header(reqwest::header::ACCEPT, "application/vnd.itch.v2")
    .send()
    .await.map_err(|e| e.to_string())?
    .json()
    .await.map_err(|e| e.to_string())?;

  match response {
    itch_types::CollectionsResponse::Success { collections } => Ok(collections),
    itch_types::CollectionsResponse::Error { errors } =>
      Err(format!(
        "The server replied with an error while trying to list the profile's collections: {}",
        errors.join("\n")
      ))
  }
}

pub async fn get_collection_games(api_key: &str, collection_id: u64) -> Result<Vec<itch_types::CollectionGame>, String> {

  let client: reqwest::Client = reqwest::Client::new();
   
  let mut games: Vec<itch_types::CollectionGame> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let response: itch_types::CollectionGamesResponse = client.get(format!("{ITCH_API_V2_BASE_URL}/collections/{collection_id}/collection-games"))
      .header(reqwest::header::AUTHORIZATION, api_key)
      .header(reqwest::header::ACCEPT, "application/vnd.itch.v2")
      .query(&[("page", page)])
      .send()
      .await.map_err(|e| e.to_string())?
      .json()
      .await.map_err(|e| e.to_string())?;
    
    let (per_page, mut collection_games) = match response {
      itch_types::CollectionGamesResponse::Success { per_page, collection_games, .. } => (per_page, collection_games),
      itch_types::CollectionGamesResponse::Error { errors } =>
        return Err(format!(
          "The server replied with an error while trying to list the collection's games: {}",
          errors.join("\n")
        ))
    };

    let num_games: u64 = collection_games.len() as u64;
    games.append(&mut collection_games);
    // Warning!!!
    // collection_games was merged into games, but it WAS NOT dropped!
    // Its length is still accessible, but this doesn't make sense!
    
    if num_games < per_page || num_games == 0 {
      break;
    }
    page += 1;
  }

  Ok(games)
}

/// Given a list of game uploads, return the url to the web game (if it exists)
/// 
/// # Arguments
/// 
/// * `uploads` - The list of uploads to search for the web version
#[allow(dead_code)]
fn get_uploads_web_game_url(uploads: Vec<itch_types::GameUpload>) -> Option<String> {
  for upload in uploads.iter() {
    if let itch_types::Type::HTML = upload.r#type {
      return Some(get_web_game_url(upload.id));
    }
  }

  None
}

/// Given an upload_id, return the url to the web game
/// 
/// # Arguments
/// 
/// * `upload_id` - The ID of the html upload
fn get_web_game_url(upload_id: u64) -> String {
  format!("https://html-classic.itch.zone/html/{upload_id}/index.html")
}