use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::time::{Instant, Duration};
use futures_util::StreamExt;
use md5::{Md5, Digest, digest::core_api::CoreWrapper};
use reqwest::{Client, Method, Response, header};
use std::path::{Path, PathBuf};
use std::borrow::Cow;
use serde::{Deserialize, Serialize};

pub mod itch_api_types;
pub mod itch_manifest;
mod heuristics;
mod game_files_operations;
mod extract;
use crate::itch_api_types::*;
use crate::game_files_operations::*;

// This isn't inside itch_types because it is not something that the itch API returns
// These platforms are *interpreted* from the data provided by the API
/// The different platforms a upload can be made for
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, clap::ValueEnum)]
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

impl Upload {
  pub fn to_game_platforms(&self) -> Vec<GamePlatform> {
    let mut platforms: Vec<GamePlatform> = Vec::new();

    match self.r#type {
      UploadType::HTML => platforms.push(GamePlatform::Web),
      UploadType::Flash => platforms.push(GamePlatform::Flash),
      UploadType::Java => platforms.push(GamePlatform::Java),
      UploadType::Unity => platforms.push(GamePlatform::UnityWebPlayer),
      _ => (),
    }

    for t in self.traits.iter() {
      match t {
        UploadTrait::PLinux => platforms.push(GamePlatform::Linux),
        UploadTrait::PWindows => platforms.push(GamePlatform::Windows),
        UploadTrait::POSX => platforms.push(GamePlatform::OSX),
        UploadTrait::PAndroid => platforms.push(GamePlatform::Android),
        _ => ()
      }
    }

    platforms
  }
}

pub enum DownloadStatus {
  Warning(String),
  StartingDownload {
    bytes_to_download: u64,
  },
  DownloadProgress {
    downloaded_bytes: u64,
  },
  Extract,
}

pub enum LaunchMethod<'a> {
  AlternativeExecutable(&'a Path),
  ManifestAction(&'a str),
  Heuristics(&'a GamePlatform, &'a Game),
}

/// Some information about a installed upload
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledUpload {
  pub upload_id: u64,
  pub game_folder: PathBuf,
  // upload and game are optional because this way, if the Game or Upload structs change
  // in the Itch's API, they can be obtained again without invalidating all previous configs
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

/// Make a request to the itch.io API
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `method` - The request method (GET, POST, etc.)
/// 
/// * `url` - A itch.io API address to make the request against
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `options` - A closure that modifies the request builder just before sending it
/// 
/// # Returns
/// 
/// The reqwest response
/// 
/// An error if sending the request fails
async fn itch_request(
  client: &Client,
  method: Method,
  url: &ItchApiUrl<'_>,
  api_key: &str,
  options: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder
) -> Result<Response, String> {
  let mut request: reqwest::RequestBuilder = client.request(method, url.to_string());

  // Add authentication based on the API's version.
  request = match url {
    // https://itchapi.ryhn.link/API/V1/index.html#authentication
    ItchApiUrl::V1(..) => request.header(header::AUTHORIZATION, format!("Bearer {api_key}")),
    // https://itchapi.ryhn.link/API/V2/index.html#authentication
    ItchApiUrl::V2(..) => request.header(header::AUTHORIZATION, api_key),
    // If it isn't a known API version, just leave it without authentication
    ItchApiUrl::Other(..) => request,
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

/// Make a request to the itch.io API and parse the response as json
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `method` - The request method (GET, POST, etc.)
/// 
/// * `url` - A itch.io API address to make the request against
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `options` - A closure that modifies the request builder just before sending it
/// 
/// # Returns
/// 
/// The reqwest response parsed as JSON into the provided type
/// 
/// An error if sending the request or parsing it fails
async fn itch_request_json<T>(
  client: &Client,
  method: Method,
  url: &ItchApiUrl<'_>,
  api_key: &str,
  options: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
) -> Result<T, String> where
  T: serde::de::DeserializeOwned,
{
  let text = itch_request(client, method, url, api_key, options).await?
    .text().await
    .map_err(|e| format!("Error while reading response body: {e}"))?;

  serde_json::from_str::<ApiResponse<T>>(&text)
    .map_err(|e| format!("Error while parsing JSON body: {e}\n\n{}", text))?
    .into_result()
}

/// Hash a file into a MD5 hasher
/// 
/// # Arguments
/// 
/// * `readable` - Anything that implements tokio::io::AsyncRead to read the data from, could be a File
/// 
/// * `hasher` - A mutable reference to a MD5 hasher, which will be updated with the file data
/// 
/// # Returns
/// 
/// An error if something goes wrong
async fn hash_readable_async(readable: impl tokio::io::AsyncRead + Unpin, hasher: &mut CoreWrapper<md5::Md5Core>) -> Result<(), String> {
  let mut br = tokio::io::BufReader::new(readable);

  loop {
    let buffer = br.fill_buf().await
      .map_err(|e| format!("Couldn't read file in order to hash it!\n{e}"))?;

    // If buffer is empty then BufReader has reached the EOF
    if buffer.is_empty() {
      break Ok(());
    }

    // Update the hasher
    hasher.update(buffer);

    // Marked the hashed bytes as read
    let len = buffer.len();
    br.consume(len);
  }
}

/// Stream a reqwest Response into a File async
/// 
/// # Arguments
/// 
/// * `response` - A file download response
/// 
/// * `file` - An opened File with write access
/// 
/// * `md5_hash` - If provided, the hasher to update with the received data
/// 
/// * `progress_callback` - A closure called with the number of downloaded bytes at the moment
/// 
/// * `callback_interval` - The minimum time span between each progress_callback call
/// 
/// # Returns
/// 
/// The total downloaded bytes
/// 
/// An error if something goes wrong
async fn stream_response_into_file(
  response: Response,
  file: &mut tokio::fs::File,
  mut md5_hash: Option<&mut CoreWrapper<md5::Md5Core>>,
  progress_callback: impl Fn(u64),
  callback_interval: Duration,
) -> Result<u64, String> {
  // Prepare the download and the callback variables
  let mut downloaded_bytes: u64 = 0;
  let mut stream = response.bytes_stream();
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
    if let Some(ref mut hasher) = md5_hash {
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

  Ok(downloaded_bytes)
}

/// Download a file from an Itch API URL
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `url` - A itch.io API address to download the file from
/// 
/// * `api_key` - A valid (or invalid, if the endpoint doesn't require it) Itch.io API key to make the request
/// 
/// * `file_path` - The path where the file will be placed
/// 
/// * `md5_hash` - A md5 hash to check the file against. If none, don't verify the download
/// 
/// * `file_size_callback` - A clousure called with total size the downloaded file will have after the download
/// 
/// * `progress_callback` - A closure called with the number of downloaded bytes at the moment
/// 
/// * `callback_interval` - The minimum time span between each progress_callback call
/// 
/// # Returns
/// 
/// A hasher, empty if update_md5_hash is false
/// 
/// An error if something goes wrong
async fn download_file(
  client: &Client,
  url: &ItchApiUrl<'_>,
  api_key: &str,
  file_path: &Path,
  md5_hash: Option<&str>,
  file_size_callback: impl Fn(u64),
  progress_callback: impl Fn(u64),
  callback_interval: Duration,
) -> Result<(), String> {

  // Create the hasher variable
  let mut md5_hash: Option<(CoreWrapper<md5::Md5Core>, &str)> = md5_hash.map(|s| (Md5::new(), s));

  // The file will be downloaded to this file with the .part extension,
  // and then the extension will be removed when the download ends
  let partial_file_path: PathBuf = add_part_extension(file_path)?;

  // If there already exists a file in file_path, then move it to partial_file_path
  // This way, the file's length and its hash are verified
  if tokio::fs::try_exists(file_path).await.map_err(|e| format!("Couldn't check is the file exists!: \"{}\"\n{e}", file_path.to_string_lossy()))? {
    tokio::fs::rename(file_path, &partial_file_path).await
      .map_err(|e| format!("Couldn't move the downloaded file:\n  Source: \"{}\"\n  Destination: \"{}\"\n{e}", file_path.to_string_lossy(), partial_file_path.to_string_lossy()))?;
  }

  // Open the file where the data is going to be downloaded
  // Use the append option to ensure that the old download data isn't deleted
  let mut file = tokio::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .read(true)
    .open(&partial_file_path).await
    .map_err(|e| format!("Couldn't open file: {}\n{e}", partial_file_path.to_string_lossy()))?;
  
  let mut downloaded_bytes: u64 = file.metadata().await
    .map_err(|e| format!("Couldn't get file metadata: {}\n{e}", partial_file_path.to_string_lossy()))?
    .len();

  let file_response: Option<Response> = 'r: {
    // Send a request for the whole file
    let res = itch_request(client, Method::GET, url, api_key, |b| b).await?;

    let download_size = res.content_length()
      .ok_or_else(|| format!("Couldn't get the Content Length of the file to download!\n{res:?}"))?;

    file_size_callback(download_size);
    
    // If the file is empty, then return the request for the whole file
    if downloaded_bytes == 0 {
      break 'r Some(res);
    }
    
    // If the file is exactly the size it should be, then return None so nothing more is downloaded
    else if downloaded_bytes == download_size {
      break 'r None;
    }

    // If the file is not empty, and smaller than the whole file, download the remaining file range
    else if downloaded_bytes < download_size {
      let part_res = itch_request(client, Method::GET, url, api_key,
        |b| b.header(header::RANGE, format!("bytes={downloaded_bytes}-"))
      ).await?;

      match part_res.status() {
        // 206 Partial Content code means the server will send the requested range
        // https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status/206
        reqwest::StatusCode::PARTIAL_CONTENT => break 'r Some(part_res),

        // 200 OK code means the server doesn't support ranges
        // https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Range
        // Don't break, so the fallback code is run instead and the whole file is downloaded
        reqwest::StatusCode::OK => (),

        // Any code other than 200 or 206 means that something went wrong
        _ => return Err(format!("The HTTP server to download the file from didn't return HTTP code 200 nor 206, so exiting! It returned: {}\n{part_res:?}", part_res.status().as_u16())),
      }
    }

    // If we're here, that means one of two things:
    //
    // 1. The file is bigger than it should
    // 2. The server doesn't support ranges
    //
    // In either case, the current file should be removed and downloaded again fully
    downloaded_bytes = 0;
    file.set_len(0).await
      .map_err(|e| format!("Couldn't remove old partially downloaded file: {}\n{e}", partial_file_path.to_string_lossy()))?;

    Some(res)
  };

  // If a partial file was already downloaded, hash the old downloaded data
  if let Some((ref mut hasher, _)) = md5_hash && downloaded_bytes > 0 {
    hash_readable_async(&mut file, hasher).await?;
  }

  // Stream the Response into the File
  if let Some(res) = file_response {
    stream_response_into_file(res, &mut file, md5_hash.as_mut().map(|(h, _)| h), |b| progress_callback(downloaded_bytes + b), callback_interval).await?;
  }

  // If the hashes aren't equal, exit with an error
  if let Some((hasher, hash)) = md5_hash {
    let file_hash = format!("{:x}", hasher.finalize());

    if !file_hash.eq_ignore_ascii_case(&hash) {
      return Err(format!("File verification failed! The file hash and the hash provided by the server are different.\n
  File hash:   {file_hash}
  Server hash: {hash}"
      ));
    }
  }

  // Sync the file to ensure all the data has been written
  file.sync_all().await
    .map_err(|e| e.to_string())?;

  // Move the downloaded file to its final destination
  // This has to be the last call in this function because after it, the File is not longer valid
  tokio::fs::rename(&partial_file_path, file_path).await
    .map_err(|e| format!("Couldn't move the downloaded file:\n  Source: \"{}\"\n  Destination: \"{}\"\n{e}", partial_file_path.to_string_lossy(), file_path.to_string_lossy()))?;

  Ok(())
}

/// Complete the login with the TOTP 2nd factor verification
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `totp_token` - The TOTP token returned by the previous login step
/// 
/// * `totp_code` - The 6-digit code returned by the TOTP application
/// 
/// # Returns
/// 
/// A LoginSuccess struct with the new API key
/// 
/// An error if something goes wrong
async fn totp_verification(client: &Client, totp_token: &str, totp_code: u64) -> Result<LoginSuccess, String> {
  itch_request_json::<LoginSuccess>(
    client,
    Method::POST,
    &ItchApiUrl::V2("totp/verify"),
    "",
    |b| b.form(&[
      ("token", totp_token),
      ("code", &totp_code.to_string())
    ]),
  ).await
    .map_err(|e| format!("An error occurred while attempting log in:\n{e}"))
}

/// Login to Itch.io
/// 
/// Retrieve a API key from a username and password authentication
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `username` - The username OR email of the accout to log in with
/// 
/// * `password` - The password of the accout to log in with
/// 
/// * `recaptcha_response` - If required, the reCAPTCHA token from https://itch.io/captcha
/// 
/// * `totp_code` - If required, The 6-digit code returned by the TOTP application
/// 
/// # Returns
/// 
/// A LoginSuccess struct with the new API key
/// 
/// An error if something goes wrong
pub async fn login(client: &Client, username: &str, password: &str, recaptcha_response: Option<&str>, totp_code: Option<u64>) -> Result<LoginSuccess, String> {
  let mut params: Vec<(&'static str, &str)> = vec![
    ("username", username),
    ("password", password),
    ("force_recaptcha", "false"),
    ("source", "desktop"),
  ];

  if let Some(rr) = recaptcha_response {
    params.push(("recaptcha_response", rr));
  }

  let response = itch_request_json::<LoginResponse>(
    client,
    Method::POST,
    &ItchApiUrl::V2("login"),
    "",
    |b| b.form(&params),
  ).await
    .map_err(|e| format!("An error occurred while attempting log in:\n{e}"))?;

  let ls = match response {
    LoginResponse::CaptchaError(e) => {
      return Err(format!(
  r#"A reCAPTCHA verification is required to continue!
  Go to "{}" and solve the reCAPTCHA.
  To obtain the token, paste the following command on the developer console:
    console.log(grecaptcha.getResponse())
  Then run the login command again with the --recaptcha-response option."#,
        e.recaptcha_url.as_str()
      ));
    }
    LoginResponse::TOTPError(e) => {
      let Some(totp_code) = totp_code else {
        return Err(format!(
  r#"The accout has 2 step verification enabled via TOTP
  Run the login command again with the --totp-code={{VERIFICATION_CODE}} option."#
        ));
      };

      totp_verification(client, e.token.as_str(), totp_code).await?
    }
    LoginResponse::Success(ls) => ls
  };

  Ok(ls)
}

/// Get the API key's profile
/// 
/// This can be used to verify that a given Itch.io API key is valid
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// # Returns
/// 
/// A User struct with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_profile(client: &Client, api_key: &str) -> Result<User, String> {
  itch_request_json::<ProfileResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2("profile"),
    api_key,
    |b| b,
  ).await
    .map(|res| res.user)
    .map_err(|e| format!("An error occurred while attempting to get the profile info:\n{e}"))
}

/// Get the user's owned game keys
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// # Returns
/// 
/// A vector of OwnedKey structs with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_owned_keys(client: &Client, api_key: &str) -> Result<Vec<OwnedKey>, String> {
  let mut keys: Vec<OwnedKey> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let mut response = itch_request_json::<OwnedKeysResponse>(
      client,
      Method::GET,
      &ItchApiUrl::V2("profile/owned-keys"),
      api_key,
      |b| b.query(&[("page", page)]),
    ).await
      .map_err(|e| format!("An error occurred while attempting to obtain the list of the user's game keys: {e}"))?;

    let num_keys: u64 = response.owned_keys.len() as u64;
    keys.append(&mut response.owned_keys);
    // Warning!!!
    // response.collection_games was merged into games, but it WAS NOT dropped!
    // Its length is still accessible, but this doesn't make sense!
    
    if num_keys < response.per_page || num_keys == 0 {
      break;
    }
    page += 1;
  }

  Ok(keys)
}

/// Get the information about a game in Itch.io
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `game_id` - The ID of the game from which information will be obtained
/// 
/// # Returns
/// 
/// A Game struct with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_game_info(client: &Client, api_key: &str, game_id: u64) -> Result<Game, String> {
  itch_request_json::<GameInfoResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2(&format!("games/{game_id}")),
    api_key,
    |b| b,
  ).await
    .map(|res| res.game)
    .map_err(|e| format!("An error occurred while attempting to obtain the game info:\n{e}"))
}

/// Get the game's uploads (downloadable files)
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `game_id` - The ID of the game from which information will be obtained
/// 
/// # Returns
/// 
/// A vector of Upload structs with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_game_uploads(client: &Client, api_key: &str, game_id: u64) -> Result<Vec<Upload>, String> {
  itch_request_json::<GameUploadsResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2(&format!("games/{game_id}/uploads")),
    api_key,
    |b| b,
  ).await
    .map(|res| res.uploads)
    .map_err(|e| format!("An error occurred while attempting to obtain the game uploads:\n{e}"))
}

/// Find out which platforms a game's uploads are available in
/// 
/// # Arguments
/// 
/// * `uploads` - A list of a game's uploads
/// 
/// # Returns
/// 
/// A vector of tuples containing an upload ID and the game platform in which it is available
pub fn get_game_platforms(uploads: &[Upload]) -> Vec<(u64, GamePlatform)> {
  let mut platforms: Vec<(u64, GamePlatform)> = Vec::new();

  for u in uploads {
    for p in u.to_game_platforms() {
      platforms.push((u.id, p));
    }
  }

  platforms
}

/// Get an upload's info
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `upload_id` - The ID of the upload from which information will be obtained
/// 
/// # Returns
/// 
/// A Upload struct with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_upload_info(client: &Client, api_key: &str, upload_id: u64) -> Result<Upload, String> {
  itch_request_json::<UploadResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2(&format!("uploads/{upload_id}")),
    api_key,
    |b| b,
  ).await
    .map(|res| res.upload)
    .map_err(|e| format!("An error occurred while attempting to obtain the upload information:\n{e}"))
}

/// List the user's game collections
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// # Returns
/// 
/// A vector of Collection structs with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_collections(client: &Client, api_key: &str) -> Result<Vec<Collection>, String> {
  itch_request_json::<CollectionsResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2("profile/collections"),
    api_key,
    |b| b,
  ).await
    .map(|res| res.collections)
    .map_err(|e| format!("An error occurred while attempting to obtain the list of the profile's collections:\n{e}"))
}

/// List a collection's games
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `collection_id` - The ID of the collection from which information will be obtained
/// 
/// # Returns
/// 
/// A vector of CollectionGameItem structs with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_collection_games(client: &Client, api_key: &str, collection_id: u64) -> Result<Vec<CollectionGameItem>, String> {   
  let mut games: Vec<CollectionGameItem> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let mut response = itch_request_json::<CollectionGamesResponse>(
      client,
      Method::GET,
      &ItchApiUrl::V2(&format!("collections/{collection_id}/collection-games")),
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

/// Download a game cover image from its game ID
/// 
/// The image will be a PNG. This is because the Itch.io servers return that type of image
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `game_id` - The ID of the game from which the cover will be downloaded
/// 
/// * `folder` - The game folder where the cover will be placed
/// 
/// * `cover_filename` - The new filename of the cover
/// 
/// * `force_download` - If true, download the cover image again, even if it already exists
/// 
/// # Returns
/// 
/// The path of the downloaded image, or None if the game doesn't have one
/// 
/// An error if something goes wrong
pub async fn download_game_cover(client: &Client, api_key: &str, game_id: u64, folder: &Path, cover_filename: Option<&str>, force_download: bool) -> Result<Option<PathBuf>, String> {
  // Get the game info from the server
  let game_info = get_game_info(client, api_key, game_id).await?;
  // If the game doesn't have a cover, return
  let Some(cover_url) = game_info.cover_url else {
    return Ok(None);
  };

  // Create the folder where the file is going to be placed if it doesn't already exist
  tokio::fs::create_dir_all(folder).await
    .map_err(|e| format!("Couldn't create the folder \"{}\": {e}", folder.to_string_lossy()))?;

  // If the cover filename isn't set, set it to "cover"
  let cover_filename = match cover_filename {
    Some(f) => f,
    None => COVER_IMAGE_DEFAULT_FILENAME,
  };

  let cover_path = folder.join(cover_filename);
  
  // If the cover image already exists and the force variable is false, don't replace the original image
  if !force_download && cover_path.try_exists().map_err(|e| format!("Couldn't check if the game cover image exists: \"{}\"\n{e}", cover_path.to_string_lossy()))? {
    return Ok(Some(cover_path));
  }

  download_file(
    client,
    &ItchApiUrl::Other(&cover_url),
    "",
    &cover_path,
    None,
    |_| (),
    |_| (),
    Duration::MAX,
  ).await?;
  
  Ok(Some(cover_path))
}

/// Download a game upload
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `upload_id` - The ID of the upload which will be downloaded
/// 
/// * `game_folder` - The folder where the downloadeded game files will be placed
/// 
/// * `upload_info` - A closure which reports the upload and the game info before the download starts
/// 
/// * `progress_callback` - A closure which reports the download progress
/// 
/// * `callback_interval` - The minimum time span between each progress_callback call
/// 
/// # Returns
/// 
/// The installation info about the upload
/// 
/// An error if something goes wrong
pub async fn download_upload(
  client: &Client,
  api_key: &str,
  upload_id: u64,
  game_folder: Option<&Path>,
  skip_hash_verification: bool,
  upload_info: impl FnOnce(&Upload, &Game),
  progress_callback: impl Fn(DownloadStatus),
  callback_interval: Duration,
) -> Result<InstalledUpload, String> {

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

  // upload_archive is the location where the upload will be downloaded
  let upload_archive: PathBuf = get_upload_archive_path(game_folder, upload_id, &upload.filename);

  // Create the game folder if it doesn't already exist
  tokio::fs::create_dir_all(&game_folder).await
    .map_err(|e| format!("Couldn't create the folder \"{}\": {e}", game_folder.to_string_lossy()))?;


  // --- DOWNLOAD --- 

  // Download the file
  download_file(
    client,
    &ItchApiUrl::V2(&format!("uploads/{upload_id}/download")),
    api_key,
    &upload_archive,
    // Only pass the hash if skip_hash_verification is false
    upload.md5_hash.as_deref().filter(|_| !skip_hash_verification),
    |bytes| progress_callback(DownloadStatus::StartingDownload { bytes_to_download: bytes } ),
    |bytes| progress_callback(DownloadStatus::DownloadProgress { downloaded_bytes: bytes } ),
    callback_interval,
  ).await?;
  
  // Print a warning if the upload doesn't have a hash in the server
  // or the hash verification is skipped
  if skip_hash_verification {
    progress_callback(DownloadStatus::Warning("Skipping hash verification! The file integrity won't be checked!".to_string()));
  } else if upload.md5_hash.is_none() {
    progress_callback(DownloadStatus::Warning("Missing md5 hash. Couldn't verify the file integrity!".to_string()));
  }


  // --- FILE EXTRACTION ---

  progress_callback(DownloadStatus::Extract);

  // The new upload_folder is game_folder + the upload id
  let upload_folder: PathBuf = get_upload_folder(game_folder, upload_id);

  // Extracts the downloaded archive (if it's an archive)
  // game_files can be the path of an executable or the path to the extracted folder
  extract::extract(&upload_archive, &upload_folder).await
    .map_err(|e| e.to_string())?;

  Ok(InstalledUpload {
    upload_id,
    // Get the absolute (canonical) form of the path
    game_folder: game_folder.canonicalize()
      .map_err(|e| format!("Error getting the canonical form of the game folder! Maybe it doesn't exist: {}\n{e}", game_folder.to_string_lossy()))?,
    upload: Some(upload),
    game: Some(game),
  })
}

/// Import an already installed upload
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid Itch.io API key to make the request
/// 
/// * `upload_id` - The ID of the upload which will be imported
/// 
/// * `game_folder` - The folder where the game files are currectly placed
/// 
/// # Returns
/// 
/// The installation info about the upload
/// 
/// An error if something goes wrong
pub async fn import(client: &Client, api_key: &str, upload_id: u64, game_folder: &Path) -> Result<InstalledUpload, String> {
  // Obtain information about the game and the upload that will be downloaeded
  let upload: Upload = get_upload_info(client, api_key, upload_id).await?;
  let game: Game = get_game_info(client, api_key, upload.game_id).await?;
  
  Ok(InstalledUpload {
    upload_id,
    // Get the absolute (canonical) form of the path
    game_folder: game_folder.canonicalize()
      .map_err(|e| format!("Error getting the canonical form of the game folder! Maybe it doesn't exist: {}\n{e}", game_folder.to_string_lossy()))?,
    upload: Some(upload),
    game: Some(game),
  })
}

/// Remove partially downloaded game files from a cancelled download
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid Itch.io API key to get info about the game to remove
/// 
/// * `upload_id` - The ID of the upload whose download was canceled
/// 
/// * `game_folder` - The folder where the game files are currectly placed
/// 
/// # Returns
/// 
/// True if something was actually deleted
/// 
/// An error if something goes wrong
pub async fn remove_partial_download(client: &Client, api_key: &str, upload_id: u64, game_folder: Option<&Path>) -> Result<bool, String> {
  // Obtain information about the game and the upload
  let upload: Upload = get_upload_info(client, api_key, upload_id).await?;
  let game: Game = get_game_info(client, api_key, upload.game_id).await?;

  // If the game_folder is unset, set it to ~/Games/{game_name}/
  let game_folder = match game_folder {
    Some(f) => f,
    None => &get_game_folder(&game.title)?,
  };

  // Vector of files and folders to be removed
  let to_be_removed_folders: &[PathBuf] = &[
    // **Do not remove the upload folder!**

    // The upload partial folder
    // Example: ~/Games/ExampleGame/123456.part/
    add_part_extension(get_upload_folder(game_folder, upload_id))?,
  ];

  let to_be_removed_files: &[PathBuf] = {
    let upload_archive  = get_upload_archive_path(game_folder, upload_id, &upload.filename);

    &[
      // The upload partial archive
      // Example: ~/Games/ExampleGame/123456-download-ArchiveName.zip.part
      add_part_extension(&upload_archive)?,

      // The upload downloaded archive
      // Example: ~/Games/ExampleGame/123456-download-ArchiveName.zip
      upload_archive,
    ]
  };

  // Set this variable to true if some file or folder was deleted
  let mut was_something_deleted: bool = false;

  // Remove the partially downloaded files
  for f in to_be_removed_files {
    if f.try_exists().map_err(|e| format!("Couldn't check if the file exists: \"{}\"\n{e}", f.to_string_lossy()))? {
      tokio::fs::remove_file(f).await
        .map_err(|e| format!("Couldn't remove file: \"{}\"\n{e}", f.to_string_lossy()))?;

      was_something_deleted = true;
    }
  }

  // Remove the partially downloaded folders
  for f in to_be_removed_folders {
    if f.try_exists().map_err(|e| format!("Couldn't check if the folder exists: \"{}\"\n{e}", f.to_string_lossy()))? {
      remove_folder_safely(f).await?;

      was_something_deleted = true;
    }
  }
  
  // If the game folder is now useless, remove it
  was_something_deleted |= remove_folder_if_empty(game_folder).await?;

  Ok(was_something_deleted)
}

/// Remove an installed upload
/// 
/// # Arguments
/// 
/// * `upload_id` - The ID of upload which will be removed
/// 
/// * `game_folder` - The folder with the game files where the upload will be removed from
/// 
/// # Returns
/// 
/// An error if something goes wrong
pub async fn remove(upload_id: u64, game_folder: &Path) -> Result<(), String> {

  let upload_folder = get_upload_folder(game_folder, upload_id);

  // If there isn't a upload_folder, or it is empty, that means the game
  // has already been removed, so return Ok(())
  if is_folder_empty(&upload_folder)? {
    return Ok(())
  }

  remove_folder_safely(upload_folder).await?;
  // The upload folder has been removed

  // If the game folder is empty, remove it
  remove_folder_if_empty(game_folder).await?;

  Ok(())
}

/// Move an installed upload to a new game folder
/// 
/// # Arguments
/// 
/// * `upload_id` - The ID of upload which will be moved
/// 
/// * `src_game_folder` - The folder where the game files are currently placed
/// 
/// * `dst_game_folder` - The folder where the game files will be moved to
/// 
/// # Returns
/// 
/// The new game folder in its absolute (canonical) form
/// 
/// An error if something goes wrong
pub async fn r#move(upload_id: u64, src_game_folder: &Path, dst_game_folder: &Path) -> Result<PathBuf, String> {
  let src_upload_folder = get_upload_folder(src_game_folder, upload_id);

  // If there isn't a src_upload_folder, exit with error
  if !src_upload_folder.try_exists().map_err(|e| format!("Couldn't check if the upload folder exists: {e}"))? {
    return Err(format!("The source game folder doesn't exsit!"));
  }
  
  let dst_upload_folder = get_upload_folder(dst_game_folder, upload_id);
  // If there is a dst_upload_folder with contents, exit with error
  if !is_folder_empty(&dst_upload_folder)? {
    return Err(format!("The upload folder destination isn't empty!: \"{}\"", dst_upload_folder.to_string_lossy()));
  }
  
  // Move the upload folder
  move_folder(src_upload_folder.as_path(), dst_upload_folder.as_path()).await?;

  // If src_game_folder is empty, remove it
  remove_folder_if_empty(src_game_folder).await?;

  dst_game_folder.canonicalize()
    .map_err(|e| format!("Error getting the canonical form of the destination game folder! Maybe it doesn't exist: {}\n{e}", dst_game_folder.to_string_lossy()))
}

/// Retrieve the itch manifest from an installed upload
/// 
/// # Arguments
/// 
/// * `upload_id` - The ID of upload from which the info will be retrieved
/// 
/// * `game_folder` - The folder with the game files where the upload folder is placed
/// 
/// # Returns
/// 
/// A Manifest struct with the manifest actions info, or None if the manifest isn't present
/// 
/// An error if something goes wrong
pub async fn get_upload_manifest(upload_id: u64, game_folder: &Path) -> Result<Option<itch_manifest::Manifest>, String> {
  let upload_folder = get_upload_folder(game_folder, upload_id);

  itch_manifest::read_manifest(&upload_folder)
}

/// Launchs an installed upload
/// 
/// # Arguments
/// 
/// * `upload_id` - The ID of upload which will be launched
/// 
/// * `game_folder` - The folder where the game uploads are placed
/// 
/// * `launch_action` - The name of the launch action in the upload folder's itch manifest
/// 
/// * `heuristics_info` - Some info required to guess which file is the upload executable
/// 
/// * `upload_executable` - Instead of heuristics_info, provide the path to the upload executable file
/// 
/// * `wrapper` - A list of a wrapper and its options to run the game with
/// 
/// * `game_arguments` - A list of arguments to launch the upload executable with
/// 
/// * `launch_start_callback` - A callback triggered just before the upload executable runs, providing information about what is about to be executed.
/// 
/// # Returns
/// 
/// An error if something goes wrong
pub async fn launch(
  upload_id: u64,
  game_folder: &Path,
  launch_method: LaunchMethod<'_>,
  wrapper: &[String],
  game_arguments: &[String],
  launch_start_callback: impl FnOnce(&Path, &tokio::process::Command)
) -> Result<(), String> {
  let upload_folder: PathBuf = get_upload_folder(game_folder, upload_id);
  
  // Determine the upload executable and its launch arguments from the function arguments, manifest, or heuristics.
  let (upload_executable, game_arguments): (&Path, Cow<[String]>) = match launch_method {
    // 1. If the launch method is an alternative executable, then that executable with the arguments provided to the function
    LaunchMethod::AlternativeExecutable(p) => (p, Cow::Borrowed(game_arguments)),
    // 2. If the launch method is a manifest action, use its executable
    LaunchMethod::ManifestAction(a) => {
      let ma = itch_manifest::launch_action(&upload_folder, Some(a))?
        .ok_or_else(|| format!("The provided launch action doesn't exist in the manifest: {a}"))?;
      (
        &PathBuf::from(ma.path),
        match game_arguments.is_empty(){
          // a) If the function's game arguments aren't empty, use those.
          false => Cow::Borrowed(game_arguments),
          // b) Otherwise, use the arguments from the manifest.
          true => Cow::Owned(ma.args.unwrap_or_default()),
        },
      )
    }
    // 3. Otherwise, if the launch method are the heuristics, use them to locate the executable
    LaunchMethod::Heuristics(gp, g) => {
      // But first, check if the game has a manifest with a "play" action, and use it if possible
      let mao = itch_manifest::launch_action(&upload_folder, None)?;

      match mao {
        // If the manifest has a "play" action, launch from it
        Some(ma) => (
          &PathBuf::from(ma.path),
          match game_arguments.is_empty(){
            // a) If the function's game arguments aren't empty, use those.
            false => Cow::Borrowed(game_arguments),
            // b) Otherwise, use the arguments from the manifest.
            true => Cow::Owned(ma.args.unwrap_or_default()),
          },
        ),
        // Else, now use the heuristics to determine the executable, with the function's game arguments
        None => (
          &heuristics::get_game_executable(upload_folder.as_path(), gp, g).await?,
          Cow::Borrowed(game_arguments),
        )
      }
    }
  };
  
  let upload_executable = upload_executable.canonicalize()
    .map_err(|e| format!("Error getting the canonical form of the upload executable path! Maybe it doesn't exist: {}\n{e}", upload_executable.to_string_lossy()))?;

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
    .args(game_arguments.as_ref());

  launch_start_callback(upload_executable.as_path(), &game_process);

  let mut child = game_process.spawn()
    .map_err(|e| {
      // Error code 8: Exec format error
      if let Some(8) = e.raw_os_error() {
        format!("Couldn't spawn the child process because it is not an executable format for this OS\n\
          Maybe a wrapper is missing or the selected game executable isn't the correct one!")
      } else {
        format!("Couldn't spawn the child process: {e}")
      }
    })?;

  child.wait().await
    .map_err(|e| format!("Error while awaiting for child exit!: {e}"))?;

  Ok(())
}

/// Get the url to a itch.io web game
/// 
/// # Arguments
/// 
/// * `upload_id` - The ID of the html upload
/// 
/// # Returns
/// 
/// The web game URL
pub fn get_web_game_url(upload_id: u64) -> String {
  format!("https://html-classic.itch.zone/html/{upload_id}/index.html")
}
