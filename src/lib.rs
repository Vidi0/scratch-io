use tokio::io::AsyncWriteExt;
use tokio::time::{Instant, Duration};
use futures_util::StreamExt;
use md5::{Md5, Digest};
use reqwest::{Client, Method, Response, header};
use std::path::{Path, PathBuf};
use flate2::read::GzDecoder;
use std::fs::{File};

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

  // --- DOWNLOAD PREPARATION --- 

  // This is a log which will be returned if the download is successful
  let mut output_log: String = String::new();

  // Obtain information about the game and the upload that will be downloaeded
  let upload: Upload = get_upload_info(client, api_key, upload_id).await?;
  let game: Game = get_game_info(client, api_key, upload.game_id).await?;
  
  // Send to the caller the game and the upload info
  upload_info(&upload, &game);

  // Set the folder and the file variables  
  // If the folder is unset, set it to ~/Games/{game_name}/
  let folder = match folder {
    Some(f) => f,
    None => &get_game_folder(game.title)?,
  };

  // Create the folder if it doesn't already exist
  tokio::fs::create_dir_all(folder).await
    .map_err(|e| format!("Couldn't create the folder {}: {e}", folder.to_string_lossy()))?;

  // The new path is folder + the filename
  let path: PathBuf = folder.join(upload.filename);
  


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
      progress_callback(downloaded_bytes);
    }
  }

  // Check the md5 hash
  match upload.md5_hash {
    None => output_log.push_str("Missing md5 hash. Couldn't verify the file integrity!\n"),
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

  // Extracts the downloaded archive (if it's an archive)
  // game_files can be the path of an executable or the path to the extracted folder
  let game_files = UploadArchive::from_file(&path)
    .extract().await
    .map_err(|e| e.to_string())?;

  Ok((game_files, output_log))
}

fn move_folder_child(folder: &Path) -> Result<(), String> {
  let child_entries = std::fs::read_dir(&folder)
    .map_err(|e| e.to_string())?;

  // move its children up one level
  // if the children is a folder with the same name,
  // call this function recursively on that folder
  for child in child_entries {
    let child = child
      .map_err(|e| e.to_string())?;
    let from = child.path();
    let to = folder.parent()
      .ok_or(format!("Error getting parent of: {:?}", &folder))?
      .join(child.file_name());

    if !to.try_exists().map_err(|e| e.to_string())? {
      std::fs::rename(&from, &to)
        .map_err(|e| e.to_string())?;
    } else {
      move_folder_child(&from)?;
    }
  }

  // remove wrapper dir
  // it might not be empty if it had a folder with the same name
  // inside, due to the function calling itself
  if folder.read_dir().map_err(|e| e.to_string())?.next().is_none() {
    std::fs::remove_dir(&folder)
      .map_err(|e| e.to_string())?;
  }

  Ok(())
}

// TODO: this function is recursive, but it calls move_folder_child, which also is.
// I don't think it's ideal to have a recursive function inside another...
fn remove_root_folder(folder: &Path) -> Result<(), String> {
  loop {
    // list entries
    let mut entries: std::fs::ReadDir = std::fs::read_dir(folder)
      .map_err(|e| e.to_string())?;

    // first entry (or empty)
    let first = match entries.next() {
      None => return Ok(()),
      Some(v) => v.map_err(|e| e.to_string())?,
    };

    // if there’s another entry, stop (not a single root)
    // if the entry is a file, also stop
    if entries.next().is_some() || first.path().is_file() {
      return Ok(());
    }

    // At this point, we know that first.path() is the wrapper dir
    move_folder_child(&first.path())?;

    // loop again in case we had nested single-root dirs
  }
}

pub struct UploadArchive {
  file: PathBuf,
  format: UploadArchiveFormat,
}

pub enum UploadArchiveFormat {
  Zip,
  TarGz,
  Other,
}

impl UploadArchive {
  fn file_without_extension(&self) -> String {
    let stem = self.file.file_stem()
      .expect("Empty filename?")
      .to_string_lossy()
      .to_string();

    match self.format {
      UploadArchiveFormat::TarGz => {
        Path::new(&stem).file_stem()
          .expect("The .tar.gz file doesn't have two extensions?")
          .to_string_lossy()
          .to_string()
      },
      _ => {
        stem
      },
    }
  }

  /// Gets the archive format of the file
  /// 
  /// If the file is not an archive, then the format is `UploadArchiveFormat::Other`
  pub fn from_file(file: &Path) -> Self {
    let Some(extension) = file.extension().map(|e| e.to_string_lossy()) else {
      return UploadArchive { file: file.to_path_buf(), format: UploadArchiveFormat::Other }
    };

    let is_tar: bool = file.file_stem()
      .expect("Empty filename?")
      .to_string_lossy()
      .to_lowercase()
      .ends_with(".tar");

    let format = if extension.eq_ignore_ascii_case("zip") {
      UploadArchiveFormat::Zip
    } else if is_tar && extension.eq_ignore_ascii_case("gz") {
      UploadArchiveFormat::TarGz
    } else {
      UploadArchiveFormat::Other
    };

    UploadArchive { file: file.to_path_buf(), format }
  }

  async fn remove(&self) -> Result<(), String> {
    tokio::fs::remove_file(&self.file).await
      .map_err(|e| e.to_string())
  }

  /// Extracts the archive into a folder with the same name (without the extension)
  /// 
  /// This function can return a path to a file (if it's not a valid archive) or to the extracted folder
  pub async fn extract(self) -> Result<PathBuf, String> {
    // If the file isn't an archive, return now
    if let UploadArchiveFormat::Other = self.format {
      return Ok(self.file);
    }

    let file = File::open(&self.file)
      .map_err(|e| e.to_string())?;

    let folder = self.file
      .parent()
      .unwrap()
      .join(self.file_without_extension());

    // If the directory exists and isn't empty, return an error
    if folder.is_dir() {
      if folder.read_dir().map_err(|e| e.to_string())?.next().is_some() {
        return Err(format!("Game folder directory isn't empty!: {}", folder.to_string_lossy()));
      }
    }

    match self.format {
      UploadArchiveFormat::Other => {
        panic!("If the format is Other, we should've exited before!");
      }
      UploadArchiveFormat::Zip => {
        let mut archive = zip::ZipArchive::new(&file)
          .map_err(|e| e.to_string())?;

        archive.extract(&folder)
          .map_err(|e| format!("Error extracting ZIP archive: {e}"))?;
      }
      UploadArchiveFormat::TarGz => {
        let gz_decoder: GzDecoder<&File> = GzDecoder::new(&file);
        let mut tar_decoder: tar::Archive<GzDecoder<&File>> = tar::Archive::new(gz_decoder);

        tar_decoder.unpack(&folder)
          .map_err(|e| format!("Error extracting tar.gz archive: {e}"))?;
      }
    }

    // If the game folder has a common root folder, remove it
    remove_root_folder(&folder)?;

    // Remove the archive
    self.remove().await?;
    Ok(folder)
  }
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
