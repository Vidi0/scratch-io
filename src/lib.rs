use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::io::AsyncReadExt;
use md5::{Md5, Digest};

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

/// Gets an upload's info
/// 
/// # Arguments
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `upload_id` - The ID of the upload from which information will be obtained
pub async fn get_upload_info(api_key: &str, upload_id: u64) -> Result<itch_types::GameUpload, String> {
    
  let client: reqwest::Client = reqwest::Client::new();

  let response: itch_types::UploadsResponse = client.get(format!("{ITCH_API_V2_BASE_URL}/uploads/{upload_id}"))
    .header(reqwest::header::AUTHORIZATION, api_key)
    .header(reqwest::header::ACCEPT, "application/vnd.itch.v2")
    .send()
    .await.map_err(|e| e.to_string())?
    .json()
    .await.map_err(|e| e.to_string())?;

  match response {
    itch_types::UploadsResponse::Success { upload } => Ok(upload),
    itch_types::UploadsResponse::Error { errors } =>
      Err(format!(
        "The server replied with an error while trying to get the upload info: {}",
        errors.join("\n")
      ))
  }
}

/// Lists the collections of games of the user
/// 
/// # Arguments
/// 
/// * `api_key` - A valid Itch.io API key to make the request
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

/// Lists the games inside a collection
/// 
/// # Arguments
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `collection_id` - The ID of the collection from which the games will be retrieved 
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

/// Downloads a upload from Itch.io
/// 
/// # Arguments
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `upload_id` - The ID of upload which will be downloaded
/// 
/// * `folder` - The folder where the downloaded file will be placed
/// 
/// * `progress_callback` - A callback function which reports the download progress
pub async fn download_upload<F, G>(api_key: &str, upload_id: u64, folder: &std::path::Path, mut file_size: F, mut progress_callback: G) -> Result<std::path::PathBuf, String> where
  F: FnMut(u64),
  G: FnMut(u64),
{
  // Obtain information about the upload that will be downloaeded
  let upload: itch_types::GameUpload = match get_upload_info(api_key, upload_id).await {
    Ok(u) => u,
    Err(e) => {
      eprintln!("Error while getting the upload's info:\n{}", e);
      std::process::exit(1);
    }
  };
  
  // Send to the caller the file size
  file_size(upload.size);

  // The new path is folder + the filename
  let path: std::path::PathBuf = folder.join(upload.filename);

  // Download the file
  let client: reqwest::Client = reqwest::Client::new();

  let file_response: reqwest::Response = client.get(format!("{ITCH_API_V2_BASE_URL}/uploads/{upload_id}/download"))
    .header(reqwest::header::AUTHORIZATION, api_key)
    .header(reqwest::header::ACCEPT, "application/vnd.itch.v2")
    .send()
    .await.map_err(|e| e.to_string())?;

  let mut downloaded_bytes: u64 = 0;
  let mut file = tokio::fs::File::create(&path)
    .await.map_err(|e| e.to_string())?;
  let mut stream = file_response.bytes_stream();

  use futures_util::StreamExt;

  // Save chunks to the file async
  while let Some(chunk) = stream.next().await {
    let chunk = match chunk {
      Ok(c) => c,
      Err(e) => {
        return Err(e.to_string());
      }
    };
    file.write_all(&chunk)
      .await.map_err(|e| e.to_string())?;
    downloaded_bytes += chunk.len() as u64;
    progress_callback(downloaded_bytes);
  }

  // Check the md5 hash
  match upload.md5_hash {
    None => {
      println!("Missing md5 hash. Couldn't verify the file integrity!");
    }
    Some(upload_hash) => {
      verify_md5_hash_file(&path, &upload_hash).await?;
    }
  }

  Ok(path)
}

/// Checks that a file md5 hash is the same as `hash`
/// 
/// # Arguments
/// 
/// * `path` - The path to the file
/// 
/// * `hash` - The md5 hash string
async fn verify_md5_hash_file(path: &std::path::Path, hash: &str) -> Result<(), String> {

  let mut file = File::open(path)
    .await.map_err(|e| e.to_string())?;
  let mut hasher = Md5::new();
  let mut buffer: [u8; 8192] = [0u8; 8192];
  
  loop {
    let n: usize = file.read(&mut buffer)
      .await.map_err(|e| e.to_string())?;
    if n == 0 {
      break; // EOF
    }
    hasher.update(&buffer[..n]);
  }
    
  let hash_string = format!("{:x}", hasher.finalize());

  if !hash_string.eq_ignore_ascii_case(&hash) {
    Err(format!("File verification failed! The file hash and the hash provided by the server are different.\n\
File hash:   {hash_string}
Server hash: {hash}\
    "))
  }
  else {
    Ok(())
  }

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
