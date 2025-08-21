use tokio::io::AsyncWriteExt;
use tokio::time::{Instant, Duration};
use futures_util::StreamExt;
use md5::{Md5, Digest};
use reqwest::{Client, Method, Response, header};
use std::path::{Path, PathBuf};

pub mod itch_types;
use crate::itch_types::*;

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
pub async fn download_upload<F, G>(client: &Client, api_key: &str, upload_id: u64, folder: Option<&Path>, upload_info: F, progress_callback: G, callback_interval: Duration) -> Result<(PathBuf, String), String> where
  F: Fn(&Upload, &Game),
  G: Fn(u64),
{
  // This is a log which will be returned if the download is successful
  let mut output_log: String = String::new();

  // Obtain information about the game and the upload that will be downloaeded
  let upload: Upload = get_upload_info(client, api_key, upload_id).await?;
  let game: Game = get_game_info(client, api_key, upload.game_id).await?;
  
  // Send to the caller the game and the upload info
  upload_info(&upload, &game);
  
  // Start the download, but don't save it to a file yet
  let file_response = itch_request(
    client,
    Method::GET,
    &ItchApiUrl::V2(format!("uploads/{upload_id}/download")),
    api_key,
    |b| b
  ).await?;

  // Set the folder and the file variables  
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

  // Create the folder if it doesn't exist
  if !folder.exists() {
    tokio::fs::create_dir_all(folder).await
      .map_err(|e| format!("Couldn't create the folder {}: {e}", folder.to_string_lossy()))?;
  }

  // The new path is folder + the filename
  let path: PathBuf = folder.join(upload.filename);
  
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
    let chunk = match chunk {
      Ok(c) => c,
      Err(e) => {
        return Err(e.to_string());
      }
    };

    // Write the chunk to the file
    file.write_all(&chunk).await
      .map_err(|e| e.to_string())?;

    // If the upload has a md5 hash, update the hasher
    if let Some(_) = upload.md5_hash {
      hasher.update(&chunk);
    }
  
    // Send a callback with the progress
    downloaded_bytes += chunk.len() as u64;
    if last_callback.elapsed() > callback_interval {
      last_callback = Instant::now();
      progress_callback(downloaded_bytes);
    }
  }

  // Check the md5 hash
  match upload.md5_hash {
    None => {
      output_log.push_str("Missing md5 hash. Couldn't verify the file integrity!\n");
    }
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

  // Extracts the downloaded archive (if it's an archive)
  // game_files can be the path of an executable or the path to the extracted folder
  let game_files = UploadArchive::from_file(&path)
    .extract().await
    .map_err(|e| e.to_string())?;

  Ok((game_files, output_log))
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
