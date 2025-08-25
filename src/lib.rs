use tokio::io::AsyncWriteExt;
use tokio::time::{Instant, Duration};
use futures_util::StreamExt;
use md5::{Md5, Digest};
use reqwest::{Client, Method, Response, header};
use std::path::{Path, PathBuf};

pub mod extract;
pub mod itch_types;
use crate::itch_types::*;

// This isn't inside itch_types because it is not something that the itch API returns
// These platforms are interpreted from the data provided by the API
#[derive(serde::Serialize)]
pub enum GamePlatforms {
  Linux,
  Windows,
  OSX,
  Android,
  Web,
  Flash,
  Java,
  UnityWebPlayer,
}

impl std::fmt::Display for GamePlatforms {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", serde_json::to_string(&self).unwrap())
  }
}

async fn itch_request<O>(client: &Client, method: Method, url: &ItchApiUrl, api_key: &str, options: O) -> Result<Response, String> where 
  O: FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
{
  let mut request: reqwest::RequestBuilder = client.request(method, url.to_string());

  request = match url {
    // https://itchapi.ryhn.link/API/V1/index.html#authentication
    ItchApiUrl::V1(..) => request.header(header::AUTHORIZATION, format!("Bearer {api_key}")),
    // https://itchapi.ryhn.link/API/V2/index.html#authentication
    ItchApiUrl::V2(..) => request.header(header::AUTHORIZATION, api_key),
  };
  // This header is set to ensure the use of the v2 version
  // https://itchapi.ryhn.link/API/V2/index.html
  if let ItchApiUrl::V2(_) = url {
    request = request.header(header::ACCEPT, "application/vnd.itch.v2");
  }

  // The callback is the final option before sending because
  // it needs to be able to modify anything
  request = options(request);

  request.send()
    .await.map_err(|e| format!("Error while sending request: {e}"))
}

async fn itch_request_json<T, O>(client: &Client, method: Method, url: &ItchApiUrl, api_key: &str, options: O) -> Result<T, String> where
  T: serde::de::DeserializeOwned,
  O: FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
{
  let text = itch_request(client, method, url, api_key, options)
    .await?
    .text()
    .await.map_err(|e| format!("Error while reading response body: {e}"))?;

  serde_json::from_str::<ApiResponse<T>>(&text)
    .map_err(|e| format!("Error while parsing JSON body: {e}\n\n{}", text))?
    .into_result()
}

/// Gets the API's profile
/// 
/// This can be used to verify that a given Itch.io API key is valid
/// 
/// # Arguments
///
/// * `api_key` - The api_key to verify against the Itch.io servers
pub async fn get_profile(client: &Client, api_key: &str) -> Result<User, String> {
  itch_request_json::<ProfileResponse, _>(
    client,
    Method::GET,
    &ItchApiUrl::V2(format!("profile")),
    api_key,
    |b| b
  ).await
    .map(|res| res.user)
    .map_err(|e| format!("An error occurred while attempting to get the profile info:\n{e}"))
}

/// Gets the information about a game in Itch.io
/// 
/// # Arguments
///
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `game_id` - The ID of the game from which information will be obtained
pub async fn get_game_info(client: &Client, api_key: &str, game_id: u64) -> Result<Game, String> {
  itch_request_json::<GameInfoResponse, _>(
    client,
    Method::GET,
    &ItchApiUrl::V2(format!("games/{game_id}")),
    api_key,
    |b| b
  ).await
    .map(|res| res.game)
    .map_err(|e| format!("An error occurred while attempting to obtain the game info:\n{e}"))
}

/// Gets the game's uploads (downloadable files)
/// 
/// # Arguments
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `game_id` - The ID of the game from which information will be obtained
pub async fn get_game_uploads(client: &Client, api_key: &str, game_id: u64) -> Result<Vec<Upload>, String> {
  itch_request_json::<GameUploadsResponse, _>(
    client,
    Method::GET,
    &ItchApiUrl::V2(format!("games/{game_id}/uploads")),
    api_key,
    |b| b
  ).await
    .map(|res| res.uploads)
    .map_err(|e| format!("An error occurred while attempting to obtain the game uploads:\n{e}"))
}

/// Gets the information about a game in Itch.io
/// 
/// # Arguments
///
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `game_id` - The ID of the game from which information will be obtained
pub fn get_game_platforms(uploads: Vec<&Upload>) -> Vec<(u64, GamePlatforms)> {
  let mut platforms: Vec<(u64, GamePlatforms)> = Vec::new();

  for u in uploads {
    match u.r#type {
      UploadType::Flash => platforms.push((u.id, GamePlatforms::Flash)),
      UploadType::Java => platforms.push((u.id, GamePlatforms::Java)),
      UploadType::Unity => platforms.push((u.id, GamePlatforms::UnityWebPlayer)),
      _ => (),
    }

    for t in u.traits.iter() {
      match t {
        UploadTrait::PLinux => platforms.push((u.id, GamePlatforms::Linux)),
        UploadTrait::PWindows => platforms.push((u.id, GamePlatforms::Windows)),
        UploadTrait::POSX => platforms.push((u.id, GamePlatforms::OSX)),
        UploadTrait::PAndroid => platforms.push((u.id, GamePlatforms::Android)),
        _ => ()
      }
    }
  }

  platforms
}

/// Gets an upload's info
/// 
/// # Arguments
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `upload_id` - The ID of the upload from which information will be obtained
pub async fn get_upload_info(client: &Client, api_key: &str, upload_id: u64) -> Result<Upload, String> {
  itch_request_json::<UploadResponse, _>(
    client,
    Method::GET,
    &ItchApiUrl::V2(format!("uploads/{upload_id}")),
    api_key,
    |b|b
  ).await
    .map(|res| res.upload)
    .map_err(|e| format!("An error occurred while attempting to obtain the upload information:\n{e}"))
}

/// Lists the collections of games of the user
/// 
/// # Arguments
/// 
/// * `api_key` - A valid Itch.io API key to make the request
pub async fn get_collections(client: &Client, api_key: &str) -> Result<Vec<Collection>, String> {
  itch_request_json::<CollectionsResponse, _>(
    client,
    Method::GET,
    &ItchApiUrl::V2(format!("profile/collections")),
    api_key,
    |b|b
  ).await
    .map(|res| res.collections)
    .map_err(|e| format!("An error occurred while attempting to obtain the list of the profile's collections:\n{e}"))
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
    let mut response = itch_request_json::<CollectionGamesResponse, _>(
      client,
      Method::GET,
      &ItchApiUrl::V2(format!("collections/{collection_id}/collection-games")),
      api_key,
      |b| b.query(&[("page", page)])
    ).await
      .map_err(|e| format!("An error occurred while attempting to obtain the list of the collection's games: {e}"))?;

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

/// The game folder is `dirs::home_dir`+`Games`+`game_title`
/// 
/// It fais if dirs::home_dir is None
fn get_game_folder(game_title: String) -> Result<PathBuf, String> {
  dirs::home_dir()
    .ok_or(String::from("Couldn't determine the home directory"))
    .map(|p| 
      p.join("Games")
        .join(game_title)
    )
}

pub enum DownloadStatus {
  Warning(String),
  Download(u64),
  Verify,
  Extract,
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
pub async fn download_upload<F, G>(client: &Client, api_key: &str, upload_id: u64, game_folder: Option<&Path>, upload_info: F, progress_callback: G, callback_interval: Duration) -> Result<PathBuf, String> where
  F: FnOnce(&Upload, &Game),
  G: Fn(DownloadStatus),
{

  // --- DOWNLOAD PREPARATION --- 

  // Obtain information about the game and the upload that will be downloaeded
  let upload: Upload = get_upload_info(client, api_key, upload_id).await?;
  let game: Game = get_game_info(client, api_key, upload.game_id).await?;
  
  // Send to the caller the game and the upload info
  upload_info(&upload, &game);

  // Set the game_folder and the file variables  
  // If the game_folder is unset, set it to ~/Games/{game_name}/
  let game_folder = match game_folder {
    Some(f) => f,
    None => &get_game_folder(game.title)?,
  };

  // Create the folder if it doesn't already exist
  tokio::fs::create_dir_all(game_folder).await
    .map_err(|e| format!("Couldn't create the folder {}: {e}", game_folder.to_string_lossy()))?;

  // The new path is game_folder + the filename
  let path: PathBuf = game_folder.join(upload.filename);

  // Check if the folder where the upload will be extracted is empty
  // This pattern matches when the function returns false, so the folder isn't empty
  if let (false, upload_folder) = extract::is_upload_folder_empty(&path)? {
    return Err(format!("Game folder directory isn't empty!: {}", upload_folder.to_string_lossy()));
  }



  // --- DOWNLOAD --- 

  // Start the download, but don't save it to a file yet
  let file_response = itch_request(
    client,
    Method::GET,
    &ItchApiUrl::V2(format!("uploads/{upload_id}/download")),
    api_key,
    |b| b
  ).await?;
  
  // Prepare the download, the hasher, and the callback variables
  let mut downloaded_bytes: u64 = 0;
  let mut file = tokio::fs::File::create(&path).await
    .map_err(|e| e.to_string())?;
  let mut stream = file_response.bytes_stream();
  let mut hasher = Md5::new();
  let mut last_callback = Instant::now();

  // Save chunks to the file async
  // Also, compute the md5 hash while it is being downloaded
  while let Some(chunk) = stream.next().await {
    // Return an error if the chunk is invalid
    let chunk = chunk
      .map_err(|e| format!("Error reading chunk: {e}"))?;

    // Write the chunk to the file
    file.write_all(&chunk).await
      .map_err(|e| format!("Error writing chunk to the file: {e}"))?;

    // If the upload has a md5 hash, update the hasher
    if upload.md5_hash.is_some() {
      hasher.update(&chunk);
    }
  
    // Send a callback with the progress
    downloaded_bytes += chunk.len() as u64;
    if last_callback.elapsed() > callback_interval {
      last_callback = Instant::now();
      progress_callback(DownloadStatus::Download(downloaded_bytes));
    }
  }

  progress_callback(DownloadStatus::Download(downloaded_bytes));

  // Check the md5 hash
  progress_callback(DownloadStatus::Verify);
  
  match upload.md5_hash {
    None => progress_callback(DownloadStatus::Warning("Missing md5 hash. Couldn't verify the file integrity!".to_string())),
    Some(upload_hash) => {
      let file_hash = format!("{:x}", hasher.finalize());

      if !file_hash.eq_ignore_ascii_case(&upload_hash) {
        return Err(format!("File verification failed! The file hash and the hash provided by the server are different.\n\
File hash:   {file_hash}
Server hash: {upload_hash}"
        ));
      }
    }
  }

  // Sync the file to ensure all the data has been written
  file.sync_all().await
    .map_err(|e| e.to_string())?;



  // --- FILE EXTRACTION ---

  progress_callback(DownloadStatus::Extract);

  // Extracts the downloaded archive (if it's an archive)
  // game_files can be the path of an executable or the path to the extracted folder
  let game_files = extract::extract(&path).await
    .map_err(|e| e.to_string())?;

  Ok(game_files)
}

/// Given a list of game uploads, return the url to the web game (if it exists)
/// 
/// # Arguments
/// 
/// * `uploads` - The list of uploads to search for the web version
#[allow(dead_code)]
fn get_uploads_web_game_url(uploads: Vec<Upload>) -> Option<String> {
  for upload in uploads.iter() {
    if let UploadType::HTML = upload.r#type {
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
