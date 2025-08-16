use tokio::fs::File;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use std::collections::HashMap;
use md5::{Md5, Digest};
use reqwest::{Client, Method, Response, header};

pub mod itch_types;
use crate::itch_types::*;

async fn itch_request(client: &Client, method: Method, url: &ItchApiUrl, parameters: Option<HashMap<&str, &str>>, api_key: &str) -> Result<Response, String> {
  let mut request: reqwest::RequestBuilder = client.request(method, url.to_string());

  request = match url {
    ItchApiUrl::V1(..) => request.header(header::AUTHORIZATION, format!("Bearer {api_key}")),
    ItchApiUrl::V2(..) => request.header(header::AUTHORIZATION, api_key),
  };
  if let Some(params) = parameters {
    request = request.query(&params);
  }
  if let ItchApiUrl::V2(_) = url {
    request = request.header(header::ACCEPT, "application/vnd.itch.v2");
  }

  request.send()
    .await.map_err(|e| format!("{e}"))
}

async fn itch_request_json<T>(client: &Client, method: Method, url: &ItchApiUrl, parameters: Option<HashMap<&str, &str>>, api_key: &str) -> Result<T, String> where
  T: serde::de::DeserializeOwned,
{
  itch_request(client, method, url, parameters, api_key)
    .await?
    .json::<ApiResponse<T>>()
    .await.map_err(|e| e.to_string())?
    .into_result()
}

/// Verifies that a given Itch.io API key is valid
/// 
/// # Arguments
///
/// * `api_key` - The api_key to verify against the Itch.io servers
pub async fn verify_api_key(client: &Client, api_key: &str) -> Result<(), String> {
  itch_request_json::<VerifyAPIKeyResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V1(format!("key/credentials/info")),
    None,
    api_key
  ).await
    .map(|_| ())
    .map_err(|e| format!("The server replied with an error while verifying the api key: {e}"))
}

/// Gets the information about a game in Itch.io
/// 
/// # Arguments
///
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `game_id` - The ID of the game from which information will be obtained
pub async fn get_game_info(client: &Client, api_key: &str, game_id: u64) -> Result<Game, String> {
  itch_request_json::<GameInfoResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2(format!("games/{game_id}")),
    None,
    api_key
  ).await
    .map(|res| res.game)
    .map_err(|e| format!("The server replied with an error while trying to get the game info: {e}"))
}

/// Gets the game's uploads (downloadable files)
/// 
/// # Arguments
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `game_id` - The ID of the game from which information will be obtained
pub async fn get_game_uploads(client: &Client, api_key: &str, game_id: u64) -> Result<Vec<GameUpload>, String> {
  itch_request_json::<GameUploadsResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2(format!("games/{game_id}/uploads")),
    None,
    api_key
  ).await
    .map(|res| res.uploads)
    .map_err(|e| format!("The server replied with an error while trying to get the game uploads: {e}"))
}

/// Gets an upload's info
/// 
/// # Arguments
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `upload_id` - The ID of the upload from which information will be obtained
pub async fn get_upload_info(client: &Client, api_key: &str, upload_id: u64) -> Result<GameUpload, String> {
  itch_request_json::<UploadResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2(format!("uploads/{upload_id}")),
    None,
    api_key
  ).await
    .map(|res| res.upload)
    .map_err(|e| format!("The server replied with an error while trying to get the upload info: {e}"))
}

/// Lists the collections of games of the user
/// 
/// # Arguments
/// 
/// * `api_key` - A valid Itch.io API key to make the request
pub async fn get_collections(client: &Client, api_key: &str) -> Result<Vec<Collection>, String> {
  itch_request_json::<CollectionsResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2(format!("profile/collections")),
    None,
    api_key
  ).await
    .map(|res| res.collections)
    .map_err(|e| format!("The server replied with an error while trying to list the profile's collections: {e}"))
}

/// Lists the games inside a collection
/// 
/// # Arguments
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `collection_id` - The ID of the collection from which the games will be retrieved 
pub async fn get_collection_games(client: &Client, api_key: &str, collection_id: u64) -> Result<Vec<CollectionGame>, String> {   
  let mut games: Vec<CollectionGame> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let mut response = itch_request_json::<CollectionGamesResponse>(
      client,
      Method::GET,
      &ItchApiUrl::V2(format!("collections/{collection_id}/collection-games")),
      Some(HashMap::from([("page", page.to_string().as_str())])),
      api_key
    ).await
      .map_err(|e| format!("The server replied with an error while trying to list the collection's games: {e}"))?;

    let num_games: u64 = response.collection_games.len() as u64;
    games.append(&mut response.collection_games);
    // Warning!!!
    // response.collection_games was merged into games, but it WAS NOT dropped!
    // Its length is still accessible, but this doesn't make sense!
    
    if num_games < response.per_page || num_games == 0 {
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
pub async fn download_upload<F, G>(client: &Client, api_key: &str, upload_id: u64, folder: Option<&std::path::Path>, mut upload_info: F, mut progress_callback: G) -> Result<(std::path::PathBuf, String), String> where
  F: FnMut(GameUpload, Game),
  G: FnMut(u64),
{
  // This is a log which will be returned if the download is successful
  let mut output_log: String = String::new();

  // Obtain information about the game and the upload that will be downloaeded
  let upload: GameUpload = get_upload_info(client, api_key, upload_id).await?;
  let game: Game = get_game_info(client, api_key, upload.game_id).await?;
  
  // Send to the caller the file size
  upload_info(upload.clone(), game.clone());

  // If the folder is unset, set it to ~/Games/{game_name}/
  let folder = match folder {
    Some(f) => f,
    None => {
      &dirs::home_dir()
        .ok_or(String::from("Couldn't determine the home directory"))?
        .join("Games")
        .join(game.title)
    }
  };

  // The new path is folder + the filename
  let path: std::path::PathBuf = folder.join(upload.filename);

  // Download the file
  let file_response = itch_request(
    client,
    Method::GET,
    &ItchApiUrl::V2(format!("uploads/{upload_id}/download")),
    None,
    api_key
  ).await?;
  
  // Create the folder if it doesn't exist
  if !folder.exists() {
    tokio::fs::create_dir_all(folder).await
      .map_err(|e| format!("Couldn't create the folder {}: {e}", folder.to_string_lossy()))?;
  }
  
  let mut downloaded_bytes: u64 = 0;
  let mut file = tokio::fs::File::create(&path).await
    .map_err(|e| e.to_string())?;
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

  file.flush().await
    .map_err(|e| e.to_string())?;

  // Check the md5 hash
  match upload.md5_hash {
    None => {
      output_log.push_str("Missing md5 hash. Couldn't verify the file integrity!\n");
    }
    Some(upload_hash) => {
      verify_md5_hash_file(&path, &upload_hash).await?;
    }
  }

  Ok((path, output_log))
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
fn get_uploads_web_game_url(uploads: Vec<GameUpload>) -> Option<String> {
  for upload in uploads.iter() {
    if let Type::HTML = upload.r#type {
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
