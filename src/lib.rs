use tokio::io::AsyncWriteExt;
use tokio::time::{Instant, Duration};
use futures_util::StreamExt;
use md5::{Md5, Digest};
use reqwest::{Client, Method, Response, header};
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

pub mod itch_api_types;
pub mod extract;
pub mod serde_rules;
use crate::itch_api_types::*;

// This isn't inside itch_types because it is not something that the itch API returns
// These platforms are interpreted from the data provided by the API
#[derive(Serialize)]
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

pub enum DownloadStatus {
  Warning(String),
  DownloadedCover(PathBuf),
  StartingDownload(),
  Download(u64),
  Extract,
}

#[derive(Serialize, Deserialize)]
pub struct InstalledUpload {
  pub upload_id: u64,
  pub game_folder: PathBuf,
  pub cover_image: Option<String>,
  pub upload: Option<Upload>,
  pub game: Option<Game>,
}

impl InstalledUpload {
  /// Returns true if the info has been updated
  pub async fn add_missing_info(&mut self, client: &Client, api_key: &str, force_update: bool) -> Result<bool, String> {
    let mut updated = false;

    if self.upload.is_none() || force_update {
      self.upload = Some(get_upload_info(client, api_key, self.upload_id).await?);
      updated = true;
    }
    if self.game.is_none() || force_update {
      self.game = Some(get_game_info(client, api_key, self.upload.as_ref().expect("The upload info has just been received. Why isn't it there?").game_id).await?);
      updated = true;
    }

    Ok(updated)
  }
}

impl std::fmt::Display for InstalledUpload {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let (u_name, u_created_at, u_updated_at, u_traits) = match self.upload.as_ref() {
      None => ("", "", "", String::new()),
      Some(u) => (
        u.display_name.as_deref().unwrap_or(&u.filename),
        u.created_at.as_str(),
        u.updated_at.as_deref().unwrap_or_default(),
        u.traits.iter().map(|t| t.to_string()).collect::<Vec<String>>().join(", "),
      )
    };

    let (g_id, g_description, g_url, g_created_at, g_published_at, a_id, a_name, a_url) = match self.game.as_ref() {
      None => (String::new(), "", "", "", "", String::new(), "", ""),
      Some(g) => (
        g.id.to_string(),
        g.short_text.as_deref().unwrap_or_default(),
        g.url.as_str(),
        g.created_at.as_str(),
        g.published_at.as_deref().unwrap_or_default(),
        g.user.id.to_string(),
        g.user.display_name.as_deref().unwrap_or(&g.user.username),
        g.user.url.as_str(),
      )
    };

    write!(f, "\
Upload id: {}
  Game folder: {}
    Cover image: {}
  Upload:
    Name: {u_name}
    Created at: {u_created_at}
    Updated at: {u_updated_at}
    Traits: {u_traits}
  Game:
    Id: {g_id}
    Description: {g_description}
    URL: {g_url}
    Created at: {g_created_at}
    Published at: {g_published_at}
  Author
    Id: {a_id}
    Name: {a_name}
    URL: {a_url}",
      self.upload_id,
      self.game_folder.to_string_lossy(),
      self.cover_image.as_deref().unwrap_or_default(),
    )
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

  request.send().await
    .map_err(|e| format!("Error while sending request: {e}"))
}

async fn itch_request_json<T, O>(client: &Client, method: Method, url: &ItchApiUrl, api_key: &str, options: O) -> Result<T, String> where
  T: serde::de::DeserializeOwned,
  O: FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
{
  let text = itch_request(client, method, url, api_key, options).await?
    .text().await
    .map_err(|e| format!("Error while reading response body: {e}"))?;

  serde_json::from_str::<ApiResponse<T>>(&text)
    .map_err(|e| format!("Error while parsing JSON body: {e}\n\n{}", text))?
    .into_result()
}

/// Downloads to a file given a reqwest Response
/// 
/// It returns a hasher, but if update_md5_hash is false, the hasher is empty
async fn download_file<T>(
  file_response: Response,
  file_path: &Path,
  md5_hash: Option<&str>,
  progress_callback: T,
  callback_interval: Duration,
) -> Result<(), String>
where
  T: Fn(u64)
{
  // Prepare the download, the hasher, and the callback variables
  let mut downloaded_bytes: u64 = 0;
  let mut file = tokio::fs::File::create(&file_path).await
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

    // If the file has a md5 hash, update the hasher
    if md5_hash.is_some() {
      hasher.update(&chunk);
    }
  
    // Send a callback with the progress
    downloaded_bytes += chunk.len() as u64;
    if last_callback.elapsed() > callback_interval {
      last_callback = Instant::now();
      progress_callback(downloaded_bytes);
    }
  }

  progress_callback(downloaded_bytes);

  // If the hashes aren't equal, exit with an error
  if let Some(hash) = md5_hash {
    let file_hash = format!("{:x}", hasher.finalize());

    if !file_hash.eq_ignore_ascii_case(&hash) {
      return Err(format!("File verification failed! The file hash and the hash provided by the server are different.\n\
File hash:   {file_hash}
Server hash: {hash}"
      ));
    }
  }

  // Sync the file to ensure all the data has been written
  file.sync_all().await
    .map_err(|e| e.to_string())?;

  Ok(())
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
    |b| b,
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
    |b| b,
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
    |b| b,
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
      UploadType::HTML => platforms.push((u.id, GamePlatforms::Web)),
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
    |b| b,
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
    |b| b,
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
pub async fn get_collection_games(client: &Client, api_key: &str, collection_id: u64) -> Result<Vec<CollectionGameItem>, String> {   
  let mut games: Vec<CollectionGameItem> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let mut response = itch_request_json::<CollectionGamesResponse, _>(
      client,
      Method::GET,
      &ItchApiUrl::V2(format!("collections/{collection_id}/collection-games")),
      api_key,
      |b| b.query(&[("page", page)]),
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

/// Downloads a game cover image from the cover_url to the provided folder
async fn download_game_cover(client: &Client, cover_url: &str, folder: &Path) -> Result<Option<PathBuf>, String> {
  let cover_extension = cover_url.rsplit(".").next().unwrap_or_default();
  let cover_path = folder.join(format!("cover.{cover_extension}"));
        
  if cover_path.try_exists().map_err(|e| e.to_string())? {
    return Ok(Some(cover_path));
  }

  let cover_response = client.request(Method::GET, cover_url)
    .send().await
    .map_err(|e| format!("Error while sending request: {e}"))?;

  download_file(
    cover_response,
    &cover_path,
    None,
    |_| (),
    Duration::MAX,
  ).await?;
 
  Ok(Some(cover_path))
}

/// The game folder is `dirs::home_dir`+`Games`+`game_title`
/// 
/// It fais if dirs::home_dir is None
fn get_game_folder(game_title: &str) -> Result<PathBuf, String> {
  dirs::home_dir()
    .ok_or(String::from("Couldn't determine the home directory"))
    .map(|p| 
      p.join("Games")
        .join(game_title)
    )
}

/// Removes a folder recursively, but checks if it is a dangerous path before doing so
async fn remove_folder_safely<P: AsRef<Path>>(path: P) -> Result<(), String> {
  let canonical = tokio::fs::canonicalize(path.as_ref()).await
    .map_err(|e| format!("Error getting the canonical form of the path!: {e}"))?;

  let home = dirs::home_dir()
    .ok_or(String::from("Couldn't determine the home directory"))?
    .canonicalize()
    .map_err(|e| format!("Error getting the canonical form of the path!: {e}"))?;

  if canonical == home {
    Err(String::from("Refusing to remove home directory!"))?
  }

  tokio::fs::remove_dir_all(path.as_ref()).await
    .map_err(|e| format!("Couldn't remove directory: {}\n{e}", path.as_ref().to_string_lossy()))?;

  Ok(())
}

/// Checks if a folder is empty
fn is_folder_empty<P: AsRef<Path>>(folder: P) -> Result<bool, String> {
  if folder.as_ref().is_dir() {
    if folder.as_ref().read_dir().map_err(|e| e.to_string())?.next().is_some() {
      return Ok(false);
    }
  }

  Ok(true)
}

/// Downloads a upload from Itch.io
/// 
/// # Arguments
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `upload_id` - The ID of upload which will be downloaded
/// 
/// * `game_folder` - The folder where the downloadeded game files will be placed
/// 
/// * `upload_info` - A callback function which reports the upload and the game info before the download starts
/// 
/// * `progress_callback` - A callback function which reports the download progress
/// 
/// * `callback_interval` - The Duration between the callbacks
pub async fn download_upload<F, G>(
  client: &Client,
  api_key: &str,
  upload_id: u64,
  game_folder: Option<&Path>,
  upload_info: F,
  progress_callback: G,
  callback_interval: Duration,
) -> Result<InstalledUpload, String>
where
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
    None => &get_game_folder(&game.title)?,
  };

  // The new upload_folder is game_folder + the upload id
  let upload_folder: PathBuf = game_folder.join(upload.id.to_string());

  // Check if the folder where the upload files will be placed is empty
  if !is_folder_empty(&upload_folder)? {
    return Err(format!("The upload folder isn't empty!: {}", upload_folder.to_string_lossy()));
  }
  
  // Create the folder if it doesn't already exist
  tokio::fs::create_dir_all(&upload_folder).await
    .map_err(|e| format!("Couldn't create the folder {}: {e}", upload_folder.to_string_lossy()))?;
  
  // upload_archive is the location where the upload will be downloaded
  let upload_archive: PathBuf = upload_folder.join(&upload.filename);


  // --- DOWNLOAD --- 

  // Download the cover image
  let cover_image: Option<PathBuf> = match game.cover_url {
    None => None,
    Some(ref cover_url) => {
      let cover_path: Option<PathBuf> = download_game_cover(client, cover_url, &game_folder).await?;
      if let Some(c) = cover_path.as_deref() {
        progress_callback(DownloadStatus::DownloadedCover(c.to_path_buf()));
      }
      cover_path
    }
  };

  progress_callback(DownloadStatus::StartingDownload());

  // Start the download, but don't save it to a file yet
  let file_response = itch_request(
    client,
    Method::GET,
    &ItchApiUrl::V2(format!("uploads/{upload_id}/download")),
    api_key,
    |b| b
  ).await?;

  // Download the file
  download_file(
    file_response,
    &upload_archive,
    upload.md5_hash.as_deref(),
    |bytes| progress_callback(DownloadStatus::Download(bytes)),
    callback_interval,
  ).await?;
  
  // Print a warning if the upload doesn't have a hash in the server
  if upload.md5_hash.is_none() {
    progress_callback(DownloadStatus::Warning("Missing md5 hash. Couldn't verify the file integrity!".to_string()));
  }


  // --- FILE EXTRACTION ---

  progress_callback(DownloadStatus::Extract);

  // Extracts the downloaded archive (if it's an archive)
  // game_files can be the path of an executable or the path to the extracted folder
  extract::extract(&upload_archive).await
    .map_err(|e| e.to_string())?;

  Ok(InstalledUpload {
    upload_id,
    // Get the absolute (canonical) form of the path
    game_folder: game_folder.canonicalize()
      .map_err(|e| format!("Error getting the canonical form of the path!: {e}"))?,
    cover_image: cover_image.map(|p| p.file_name().expect("Cover image doesn't have a filename?").to_string_lossy().to_string()),
    upload: Some(upload),
    game: Some(game),
  })
}

/// Downloads a upload from Itch.io
/// 
/// # Arguments
/// 
/// * `upload_id` - The ID of upload which will be downloaded
/// 
/// * `game_folder` - The folder with the game files where the upload will be removed from
pub async fn remove(upload_id: u64, game_folder: &Path) -> Result<(), String> {

  let upload_folder = game_folder.join(upload_id.to_string());
  remove_folder_safely(upload_folder).await?;
  // The upload folder has been removed

  // If there isn't another upload folder, remove the game folder
  let child_entries = std::fs::read_dir(&game_folder)
    .map_err(|e| e.to_string())?;

  for child in child_entries {
    let child = child
      .map_err(|e| e.to_string())?;

    if child.path().is_dir() {
      return Ok(())
    }
  }

  // If we're here, that means the game folder doesn't have any other
  // folders inside, so we can remove the game folder
  remove_folder_safely(game_folder).await?;

  Ok(())
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
