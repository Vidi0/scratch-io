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
pub mod heuristics;
use crate::itch_api_types::*;

// This isn't inside itch_types because it is not something that the itch API returns
// These platforms are interpreted from the data provided by the API
#[derive(Serialize, Clone, clap::ValueEnum)]
pub enum GamePlatform {
  Linux,
  Windows,
  OSX,
  Android,
  Web,
  Flash,
  Java,
  UnityWebPlayer,
}

impl std::fmt::Display for GamePlatform {
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
pub fn get_game_platforms(uploads: Vec<&Upload>) -> Vec<(u64, GamePlatform)> {
  let mut platforms: Vec<(u64, GamePlatform)> = Vec::new();

  for u in uploads {
    match u.r#type {
      UploadType::HTML => platforms.push((u.id, GamePlatform::Web)),
      UploadType::Flash => platforms.push((u.id, GamePlatform::Flash)),
      UploadType::Java => platforms.push((u.id, GamePlatform::Java)),
      UploadType::Unity => platforms.push((u.id, GamePlatform::UnityWebPlayer)),
      _ => (),
    }

    for t in u.traits.iter() {
      match t {
        UploadTrait::PLinux => platforms.push((u.id, GamePlatform::Linux)),
        UploadTrait::PWindows => platforms.push((u.id, GamePlatform::Windows)),
        UploadTrait::POSX => platforms.push((u.id, GamePlatform::OSX)),
        UploadTrait::PAndroid => platforms.push((u.id, GamePlatform::Android)),
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

fn find_cover_filename<P: AsRef<Path>>(game_folder: P) -> Result<Option<String>, String> { 
  let child_entries = std::fs::read_dir(&game_folder)
    .map_err(|e| format!("Couldn't read direcotory: {}\n{e}", game_folder.as_ref().to_string_lossy()))?;

  for child in child_entries {
    let child: std::fs::DirEntry = child
      .map_err(|e| e.to_string())?;
    let path: PathBuf = child.path();

    let Some(stem) = path.file_stem() else {
      continue;
    };

    if path.is_file() && stem.eq_ignore_ascii_case("cover")  {
      return Ok(Some(child.file_name().to_string_lossy().to_string()));
    }
  }

  Ok(None)
}

fn make_executable(path: &Path) -> Result<(), String> {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;

    let metadata = std::fs::metadata(path)
      .map_err(|e| format!("Couldn't read file metadata of {}: {e}", path.to_string_lossy()))?;
    let mut permissions = metadata.permissions();
    
    let mode = permissions.mode();
    permissions.set_mode(mode | 0o111);

    std::fs::set_permissions(path, permissions)
      .map_err(|e| format!("Couldn't set permissions of {}: {e}", path.to_string_lossy()))?;
  }

  Ok(())
}

/// Joins the upload folder and the upload id
fn get_upload_folder<P: AsRef<Path>>(game_folder: P, upload_id: u64) -> PathBuf {
  game_folder.as_ref().join(upload_id.to_string())
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
  let canonical = tokio::fs::canonicalize(&path).await
    .map_err(|e| format!("Error getting the canonical form of the path!: {e}"))?;

  let home = dirs::home_dir()
    .ok_or(String::from("Couldn't determine the home directory"))?
    .canonicalize()
    .map_err(|e| format!("Error getting the canonical form of the path!: {e}"))?;

  if canonical == home {
    Err(String::from("Refusing to remove home directory!"))?
  }

  tokio::fs::remove_dir_all(&path).await
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

/// Copy all the folder contents to another location
async fn copy_dir_all<P: AsRef<Path>>(src: P, dst: P) -> Result<(), String> {
  if !src.as_ref().is_dir() {
    return Err(format!("Not a folder: {}", src.as_ref().to_string_lossy()));
  }

  let mut queue: std::collections::VecDeque<(PathBuf, PathBuf)> = std::collections::VecDeque::new();
  queue.push_back((src.as_ref().to_path_buf(), dst.as_ref().to_path_buf()));

  while let Some((src, dst)) = queue.pop_front() {
    tokio::fs::create_dir_all(&dst).await
      .map_err(|e| format!("Couldn't create folder {}: {e}", dst.as_path().to_string_lossy()))?;

    let mut entries = tokio::fs::read_dir(&src).await
      .map_err(|e| format!("Couldn't read dir {}: {e}", src.as_path().to_string_lossy()))?;

    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
      let src_path = entry.path();
      let dst_path = dst.join(entry.file_name());

      if entry.file_type().await.map_err(|e| e.to_string())?.is_dir() {
        queue.push_back((src_path, dst_path));
      } else {
        tokio::fs::copy(&src_path, &dst_path)
          .await
          .map_err(|e| format!("Couldn't copy file:\n  from: {}\n  to: {}\n{e}", src_path.to_string_lossy(), dst_path.to_string_lossy()))?;
      } 
    }
  }

  Ok(())
}

/// This function will remove a folder AND ITS CONTENTS if it doesn't have another folder inside
async fn remove_folder_without_child_folders<P: AsRef<Path>>(folder: P) -> Result<(), String> {
  // If there isn't another folder inside, remove the folder
  let child_entries = std::fs::read_dir(&folder)
    .map_err(|e| e.to_string())?;

  for child in child_entries {
    let child = child
      .map_err(|e| e.to_string())?;

    if child.file_type().map_err(|e| e.to_string())?.is_dir() {
      return Ok(())
    }
  }

  // If we're here, that means the folder doesn't have any other
  // folder inside, so we can remove it
  remove_folder_safely(&folder).await?;

  Ok(())
}

/// Move a folder and its contents to another location
/// 
/// It also works if the destination is on another filesystem
async fn move_folder<P: AsRef<Path>>(src: P, dst: P) -> Result<(), String> {
  if !src.as_ref().is_dir() {
    Err(format!("The source folder doesn't exist!: {}", src.as_ref().to_string_lossy()))?;
  }

  // Create the destination parent dir
  tokio::fs::create_dir_all(&dst).await
    .map_err(|e| format!("Couldn't create folder: {}\n{e}", dst.as_ref().to_string_lossy()))?;

  match tokio::fs::rename(&src, &dst).await {
    Ok(_) => Ok(()),
    Err(e) if e.kind() == tokio::io::ErrorKind::CrossesDevices => {
      // fallback: copy + delete
      copy_dir_all(&src, &dst).await?;
      remove_folder_safely(&src).await?;
      Ok(())
    }
    Err(e) => Err(format!("Couldn't move the folder:\n  from: {}\n  to: {}\n{e}", src.as_ref().to_string_lossy(), dst.as_ref().to_string_lossy())),
  }
}


/// Downloads an upload from Itch.io
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
  let upload_folder: PathBuf = get_upload_folder(game_folder, upload_id);

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

/// Import an already installed upload
/// 
/// # Arguments
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `upload_id` - The ID of upload which will be imported
/// 
/// * `game_folder` - The folder where the game files are currectly placed
pub async fn import(client: &Client, api_key: &str, upload_id: u64, game_folder: &Path) -> Result<InstalledUpload, String> {
  // Obtain information about the game and the upload that will be downloaeded
  let upload: Upload = get_upload_info(client, api_key, upload_id).await?;
  let game: Game = get_game_info(client, api_key, upload.game_id).await?;
  
  let cover_image: Option<String> = find_cover_filename(game_folder)?;

  Ok(InstalledUpload {
    upload_id,
    // Get the absolute (canonical) form of the path
    game_folder: game_folder.canonicalize()
      .map_err(|e| format!("Error getting the canonical form of the path!: {e}"))?,
    cover_image,
    upload: Some(upload),
    game: Some(game),
  })
}

/// Downloads an upload from Itch.io
/// 
/// # Arguments
/// 
/// * `upload_id` - The ID of upload which will be downloaded
/// 
/// * `game_folder` - The folder with the game files where the upload will be removed from
pub async fn remove(upload_id: u64, game_folder: &Path) -> Result<(), String> {

  let upload_folder = get_upload_folder(game_folder, upload_id);

  // If there isn't a upload_folder, or it is empty, that means the game
  // has already been removed, so return Ok(())
  if is_folder_empty(&upload_folder)? {
    return Ok(())
  }

  remove_folder_safely(upload_folder).await?;
  // The upload folder has been removed

  // If there isn't another upload folder, remove the whole game folder
  remove_folder_without_child_folders(&game_folder).await?;

  Ok(())
}

/// Moves an upload to a new game folder
/// 
/// # Arguments
/// 
/// * `upload_id` - The ID of upload which will be downloaded
/// 
/// * `src_game_folder` - The folder where the game files are currently placed
/// 
/// * `dst_game_folder` - The folder where the game files will be moved to
/// 
/// * `upload_info` - If provided, modifies its contents to reflect the folder change
pub async fn r#move(upload_id: u64, src_game_folder: &Path, dst_game_folder: &Path, upload_info: Option<&mut InstalledUpload>) -> Result<(), String> {
  let src_upload_folder = get_upload_folder(src_game_folder, upload_id);

  // If there isn't a src_upload_folder, exit with error
  if !src_upload_folder.try_exists().map_err(|e| format!("Couldn't check if the upload folder exists: {e}"))? {
    return Err(format!("The source game folder doesn't exsit!"));
  }
  
  let dst_upload_folder = get_upload_folder(dst_game_folder, upload_id);
  // If there is a dst_upload_folder with contents, exit with error
  if !is_folder_empty(&dst_upload_folder)? {
    return Err(format!("The upload folder destination isn't empty!: {}", dst_upload_folder.to_string_lossy()));
  }
  
  // Move the upload folder
  move_folder(src_upload_folder.as_path(), dst_upload_folder.as_path()).await?;

  // Copy the cover image (if it exists)
  let cover_image = find_cover_filename(src_game_folder)?;
  if let Some(cover) = cover_image {
    tokio::fs::copy(src_game_folder.join(cover.as_str()), dst_game_folder.join(cover.as_str())).await
      .map_err(|e| format!("Couldn't copy game cover image: {e}"))?;
  }

  // If src_game_folder doesn't contain any other upload, remove it
  remove_folder_without_child_folders(src_game_folder).await?;

  if let Some(ui) = upload_info {
    ui.game_folder = dst_game_folder.canonicalize()
      .map_err(|e| format!("Error getting the canonical form of the path!: {e}"))?;
  }
  
  Ok(())
}

pub async fn launch(upload_id: u64, game_folder: &Path, heuristics_info: Option<(&GamePlatform, &Game, &Upload)>, upload_executable: Option<&Path>, wrapper: &[String], game_arguments: &[String], launch_start_callback: impl FnOnce(&Path, &str)) -> Result<(), String> {
  if heuristics_info.is_none() && upload_executable.is_none() {
    Err("At least one of heruristics_info or upload_executable must be set to be able to determina the game executable!")?
  }

  let upload_folder: PathBuf = get_upload_folder(game_folder, upload_id);

  // Get the upload executable file, from the arguments or from heuristics
  let upload_executable = match upload_executable {
    Some(p) => p.to_path_buf(),
    None => {
      let hi = heuristics_info.expect("We already checked if both were None!");
      heuristics::get_game_executable(upload_folder.as_path(), hi.0, &hi.1, &hi.2)?
        .ok_or_else(|| format!("Couldn't get the game executable file! Try setting one manually with the upload_executable option!"))?
    }
  }.canonicalize()
    .map_err(|e| format!("Error getting the canonical form of the path!: {e}"))?;

  // Make the file executable
  make_executable(&upload_executable)?;

  // Create the tokio process
  let mut game_process = {
    let mut wrapper_iter = wrapper.iter();
    match wrapper_iter.next() {
      // If it doesn't have a wrapper, just run the executable
      None => tokio::process::Command::new(&upload_executable),
      Some(w) => {
        // If the game has a wrapper, then run the wrapper with its
        // arguments and add the game executable as the last argument
        let mut gp = tokio::process::Command::new(w);
        gp.args(wrapper_iter.as_slice())
          .arg(&upload_executable);
        gp
      }
    }
  };

  // Add the working directory and the game arguments
  game_process.current_dir(&upload_folder)
    .args(game_arguments);

  launch_start_callback(upload_executable.as_path(), format!("{:?}", game_process).as_str());

  let mut child = game_process.spawn()
    .map_err(|e| format!("Couldn't spawn child process: {e}"))?;

  child.wait().await
    .map_err(|e| format!("Error while awaiting for child exit!: {e}"))?;

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
