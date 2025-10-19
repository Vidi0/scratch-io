mod extract;
mod game_files_operations;
mod heuristics;
pub mod itch_api;
pub mod itch_manifest;
use crate::game_files_operations::*;
pub use crate::itch_api::ItchClient;
use crate::itch_api::{types::*, *};

use futures_util::StreamExt;
use md5::{Digest, Md5, digest::core_api::CoreWrapper};
use reqwest::{Method, Response, header};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::time::{Duration, Instant};

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
  #[must_use]
  pub fn to_game_platforms(&self) -> Vec<GamePlatform> {
    let mut platforms: Vec<GamePlatform> = Vec::new();

    match self.r#type {
      UploadType::Html => platforms.push(GamePlatform::Web),
      UploadType::Flash => platforms.push(GamePlatform::Flash),
      UploadType::Java => platforms.push(GamePlatform::Java),
      UploadType::Unity => platforms.push(GamePlatform::UnityWebPlayer),
      _ => (),
    }

    for t in &self.traits {
      match t {
        UploadTrait::PLinux => platforms.push(GamePlatform::Linux),
        UploadTrait::PWindows => platforms.push(GamePlatform::Windows),
        UploadTrait::POsx => platforms.push(GamePlatform::OSX),
        UploadTrait::PAndroid => platforms.push(GamePlatform::Android),
        UploadTrait::Demo => (),
      }
    }

    platforms
  }
}

pub enum DownloadStatus {
  Warning(String),
  StartingDownload { bytes_to_download: u64 },
  DownloadProgress { downloaded_bytes: u64 },
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
  // in the itch's API, they can be obtained again without invalidating all previous configs
  pub upload: Option<Upload>,
  pub game: Option<Game>,
}

impl InstalledUpload {
  /// Returns true if the info has been updated
  pub async fn add_missing_info(
    &mut self,
    client: &ItchClient,
    force_update: bool,
  ) -> Result<bool, String> {
    let mut updated = false;

    if self.upload.is_none() || force_update {
      self.upload = Some(get_upload_info(client, self.upload_id).await?);
      updated = true;
    }
    if self.game.is_none() || force_update {
      self.game = Some(
        get_game_info(
          client,
          self
            .upload
            .as_ref()
            .expect("The upload info has just been received. Why isn't it there?")
            .game_id,
        )
        .await?,
      );
      updated = true;
    }

    Ok(updated)
  }
}

/// Hash a file into a MD5 hasher
///
/// # Arguments
///
/// * `readable` - Anything that implements `tokio::io::AsyncRead` to read the data from, could be a File
///
/// * `hasher` - A mutable reference to a MD5 hasher, which will be updated with the file data
///
/// # Returns
///
/// An error if something goes wrong
async fn hash_readable_async(
  readable: impl tokio::io::AsyncRead + Unpin,
  hasher: &mut CoreWrapper<md5::Md5Core>,
) -> Result<(), String> {
  let mut br = tokio::io::BufReader::new(readable);

  loop {
    let buffer = br
      .fill_buf()
      .await
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

/// Stream a reqwest `Response` into a `File` async
///
/// # Arguments
///
/// * `response` - A file download response
///
/// * `file` - An opened `File` with write access
///
/// * `md5_hash` - If provided, the hasher to update with the received data
///
/// * `progress_callback` - A closure called with the number of downloaded bytes at the moment
///
/// * `callback_interval` - The minimum time span between each `progress_callback` call
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
    let chunk = chunk.map_err(|e| format!("Error reading chunk: {e}"))?;

    // Write the chunk to the file
    file
      .write_all(&chunk)
      .await
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

/// Download a file from an itch API URL
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// * `url` - A itch.io API address to download the file from
///
/// * `file_path` - The path where the file will be placed
///
/// * `md5_hash` - A md5 hash to check the file against. If none, don't verify the download
///
/// * `file_size_callback` - A clousure called with total size the downloaded file will have after the download
///
/// * `progress_callback` - A closure called with the number of downloaded bytes at the moment
///
/// * `callback_interval` - The minimum time span between each `progress_callback` call
///
/// # Returns
///
/// An error if something goes wrong
async fn download_file(
  client: &ItchClient,
  url: &ItchApiUrl<'_>,
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
  if tokio::fs::try_exists(file_path).await.map_err(|e| {
    format!(
      "Couldn't check is the file exists!: \"{}\"\n{e}",
      file_path.to_string_lossy()
    )
  })? {
    tokio::fs::rename(file_path, &partial_file_path)
      .await
      .map_err(|e| {
        format!(
          "Couldn't move the downloaded file:\n  Source: \"{}\"\n  Destination: \"{}\"\n{e}",
          file_path.to_string_lossy(),
          partial_file_path.to_string_lossy()
        )
      })?;
  }

  // Open the file where the data is going to be downloaded
  // Use the append option to ensure that the old download data isn't deleted
  let mut file = tokio::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .read(true)
    .open(&partial_file_path)
    .await
    .map_err(|e| {
      format!(
        "Couldn't open file: {}\n{e}",
        partial_file_path.to_string_lossy()
      )
    })?;

  let mut downloaded_bytes: u64 = file
    .metadata()
    .await
    .map_err(|e| {
      format!(
        "Couldn't get file metadata: {}\n{e}",
        partial_file_path.to_string_lossy()
      )
    })?
    .len();

  let file_response: Option<Response> = 'r: {
    // Send a request for the whole file
    let res = client.itch_request(url, Method::GET, |b| b).await?;

    let download_size = res.content_length().ok_or_else(|| {
      format!("Couldn't get the Content Length of the file to download!\n{res:?}")
    })?;

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
      let part_res = client
        .itch_request(url, Method::GET, |b| {
          b.header(header::RANGE, format!("bytes={downloaded_bytes}-"))
        })
        .await?;

      match part_res.status() {
        // 206 Partial Content code means the server will send the requested range
        // https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status/206
        reqwest::StatusCode::PARTIAL_CONTENT => break 'r Some(part_res),

        // 200 OK code means the server doesn't support ranges
        // https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Range
        // Don't break, so the fallback code is run instead and the whole file is downloaded
        reqwest::StatusCode::OK => (),

        // Any code other than 200 or 206 means that something went wrong
        _ => {
          return Err(format!(
            "The HTTP server to download the file from didn't return HTTP code 200 nor 206, so exiting! It returned: {}\n{part_res:?}",
            part_res.status().as_u16()
          ));
        }
      }
    }

    // If we're here, that means one of two things:
    //
    // 1. The file is bigger than it should
    // 2. The server doesn't support ranges
    //
    // In either case, the current file should be removed and downloaded again fully
    downloaded_bytes = 0;
    file.set_len(0).await.map_err(|e| {
      format!(
        "Couldn't remove old partially downloaded file: {}\n{e}",
        partial_file_path.to_string_lossy()
      )
    })?;

    Some(res)
  };

  // If a partial file was already downloaded, hash the old downloaded data
  if let Some((ref mut hasher, _)) = md5_hash
    && downloaded_bytes > 0
  {
    hash_readable_async(&mut file, hasher).await?;
  }

  // Stream the Response into the File
  if let Some(res) = file_response {
    stream_response_into_file(
      res,
      &mut file,
      md5_hash.as_mut().map(|(h, _)| h),
      |b| progress_callback(downloaded_bytes + b),
      callback_interval,
    )
    .await?;
  }

  // If the hashes aren't equal, exit with an error
  if let Some((hasher, hash)) = md5_hash {
    let file_hash = format!("{:x}", hasher.finalize());

    if !file_hash.eq_ignore_ascii_case(hash) {
      return Err(format!("File verification failed! The file hash and the hash provided by the server are different.\n
  File hash:   {file_hash}
  Server hash: {hash}"
      ));
    }
  }

  // Sync the file to ensure all the data has been written
  file.sync_all().await.map_err(|e| e.to_string())?;

  // Move the downloaded file to its final destination
  // This has to be the last call in this function because after it, the File is not longer valid
  tokio::fs::rename(&partial_file_path, file_path)
    .await
    .map_err(|e| {
      format!(
        "Couldn't move the downloaded file:\n  Source: \"{}\"\n  Destination: \"{}\"\n{e}",
        partial_file_path.to_string_lossy(),
        file_path.to_string_lossy()
      )
    })?;

  Ok(())
}

/// Find out which platforms a game's uploads are available in
///
/// # Arguments
///
/// * `uploads` - A list of a game's uploads
///
/// # Returns
///
/// A vector of tuples containing an upload ID and the `GamePlatform` in which it is available
#[must_use]
pub fn get_game_platforms(uploads: &[Upload]) -> Vec<(u64, GamePlatform)> {
  let mut platforms: Vec<(u64, GamePlatform)> = Vec::new();

  for u in uploads {
    for p in u.to_game_platforms() {
      platforms.push((u.id, p));
    }
  }

  platforms
}

/// Download a game cover image from its game ID
///
/// The image will be a PNG. This is because the itch.io servers return that type of image
///
/// # Arguments
///
/// * `client` - An itch.io API client
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
/// # Errors
///
/// If something goes wrong
pub async fn download_game_cover(
  client: &ItchClient,
  game_id: u64,
  folder: &Path,
  cover_filename: Option<&str>,
  force_download: bool,
) -> Result<Option<PathBuf>, String> {
  // Get the game info from the server
  let game = get_game_info(client, game_id).await?;
  // If the game doesn't have a cover, return
  let Some(cover_url) = game.game_info.cover_url else {
    return Ok(None);
  };

  // Create the folder where the file is going to be placed if it doesn't already exist
  tokio::fs::create_dir_all(folder).await.map_err(|e| {
    format!(
      "Couldn't create the folder \"{}\": {e}",
      folder.to_string_lossy()
    )
  })?;

  // If the cover filename isn't set, set it to "cover"
  let cover_filename = match cover_filename {
    Some(f) => f,
    None => COVER_IMAGE_DEFAULT_FILENAME,
  };

  let cover_path = folder.join(cover_filename);

  // If the cover image already exists and the force variable is false, don't replace the original image
  if !force_download
    && cover_path.try_exists().map_err(|e| {
      format!(
        "Couldn't check if the game cover image exists: \"{}\"\n{e}",
        cover_path.to_string_lossy()
      )
    })?
  {
    return Ok(Some(cover_path));
  }

  download_file(
    client,
    &ItchApiUrl::Other(&cover_url),
    &cover_path,
    None,
    |_| (),
    |_| (),
    Duration::MAX,
  )
  .await?;

  Ok(Some(cover_path))
}

/// Download a game upload
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// * `upload_id` - The ID of the upload which will be downloaded
///
/// * `game_folder` - The folder where the downloadeded game files will be placed
///
/// * `skip_hash_verification` - If true, don't check the downloaded upload integrity (insecure)
///
/// * `upload_info` - A closure which reports the upload and the game info before the download starts
///
/// * `progress_callback` - A closure which reports the download progress
///
/// * `callback_interval` - The minimum time span between each `progress_callback` call
///
/// # Returns
///
/// The installation info about the upload
///
/// # Errors
///
/// If something goes wrong
pub async fn download_upload(
  client: &ItchClient,
  upload_id: u64,
  game_folder: Option<&Path>,
  skip_hash_verification: bool,
  upload_info: impl FnOnce(&Upload, &Game),
  progress_callback: impl Fn(DownloadStatus),
  callback_interval: Duration,
) -> Result<InstalledUpload, String> {
  // --- DOWNLOAD PREPARATION ---

  // Obtain information about the game and the upload that will be downloaeded
  let upload: Upload = get_upload_info(client, upload_id).await?;
  let game: Game = get_game_info(client, upload.game_id).await?;

  // Send to the caller the game and the upload info
  upload_info(&upload, &game);

  // Set the game_folder and the file variables
  // If the game_folder is unset, set it to ~/Games/{game_name}/
  let game_folder = match game_folder {
    Some(f) => f,
    None => &get_game_folder(&game.game_info.title)?,
  };

  // upload_archive is the location where the upload will be downloaded
  let upload_archive: PathBuf = get_upload_archive_path(game_folder, upload_id, &upload.filename);

  // Create the game folder if it doesn't already exist
  tokio::fs::create_dir_all(&game_folder).await.map_err(|e| {
    format!(
      "Couldn't create the folder \"{}\": {e}",
      game_folder.to_string_lossy()
    )
  })?;

  // --- DOWNLOAD ---

  // Download the file
  download_file(
    client,
    &ItchApiUrl::V2(&format!("uploads/{upload_id}/download")),
    &upload_archive,
    // Only pass the hash if skip_hash_verification is false
    upload
      .md5_hash
      .as_deref()
      .filter(|_| !skip_hash_verification),
    |bytes| {
      progress_callback(DownloadStatus::StartingDownload {
        bytes_to_download: bytes,
      });
    },
    |bytes| {
      progress_callback(DownloadStatus::DownloadProgress {
        downloaded_bytes: bytes,
      });
    },
    callback_interval,
  )
  .await?;

  // Print a warning if the upload doesn't have a hash in the server
  // or the hash verification is skipped
  if skip_hash_verification {
    progress_callback(DownloadStatus::Warning(
      "Skipping hash verification! The file integrity won't be checked!".to_string(),
    ));
  } else if upload.md5_hash.is_none() {
    progress_callback(DownloadStatus::Warning(
      "Missing md5 hash. Couldn't verify the file integrity!".to_string(),
    ));
  }

  // --- FILE EXTRACTION ---

  progress_callback(DownloadStatus::Extract);

  // The new upload_folder is game_folder + the upload id
  let upload_folder: PathBuf = get_upload_folder(game_folder, upload_id);

  // Extracts the downloaded archive (if it's an archive)
  // game_files can be the path of an executable or the path to the extracted folder
  extract::extract(&upload_archive, &upload_folder)
    .await
    .map_err(|e| e.to_string())?;

  Ok(InstalledUpload {
    upload_id,
    // Get the absolute (canonical) form of the path
    game_folder: game_folder.canonicalize().map_err(|e| {
      format!(
        "Error getting the canonical form of the game folder! Maybe it doesn't exist: {}\n{e}",
        game_folder.to_string_lossy()
      )
    })?,
    upload: Some(upload),
    game: Some(game),
  })
}

/// Import an already installed upload
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// * `upload_id` - The ID of the upload which will be imported
///
/// * `game_folder` - The folder where the game files are currectly placed
///
/// # Returns
///
/// The installation info about the upload
///
/// # Errors
///
/// If something goes wrong
pub async fn import(
  client: &ItchClient,
  upload_id: u64,
  game_folder: &Path,
) -> Result<InstalledUpload, String> {
  // Obtain information about the game and the upload that will be downloaeded
  let upload: Upload = get_upload_info(client, upload_id).await?;
  let game: Game = get_game_info(client, upload.game_id).await?;

  Ok(InstalledUpload {
    upload_id,
    // Get the absolute (canonical) form of the path
    game_folder: game_folder.canonicalize().map_err(|e| {
      format!(
        "Error getting the canonical form of the game folder! Maybe it doesn't exist: {}\n{e}",
        game_folder.to_string_lossy()
      )
    })?,
    upload: Some(upload),
    game: Some(game),
  })
}

/// Remove partially downloaded game files from a cancelled download
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// * `upload_id` - The ID of the upload whose download was canceled
///
/// * `game_folder` - The folder where the game files are currectly placed
///
/// # Returns
///
/// True if something was actually deleted
///
/// # Errors
///
/// If something goes wrong
pub async fn remove_partial_download(
  client: &ItchClient,
  upload_id: u64,
  game_folder: Option<&Path>,
) -> Result<bool, String> {
  // Obtain information about the game and the upload
  let upload: Upload = get_upload_info(client, upload_id).await?;
  let game: Game = get_game_info(client, upload.game_id).await?;

  // If the game_folder is unset, set it to ~/Games/{game_name}/
  let game_folder = match game_folder {
    Some(f) => f,
    None => &get_game_folder(&game.game_info.title)?,
  };

  // Vector of files and folders to be removed
  let to_be_removed_folders: &[PathBuf] = &[
    // **Do not remove the upload folder!**

    // The upload partial folder
    // Example: ~/Games/ExampleGame/123456.part/
    add_part_extension(get_upload_folder(game_folder, upload_id))?,
  ];

  let to_be_removed_files: &[PathBuf] = {
    let upload_archive = get_upload_archive_path(game_folder, upload_id, &upload.filename);

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
    if f.try_exists().map_err(|e| {
      format!(
        "Couldn't check if the file exists: \"{}\"\n{e}",
        f.to_string_lossy()
      )
    })? {
      tokio::fs::remove_file(f)
        .await
        .map_err(|e| format!("Couldn't remove file: \"{}\"\n{e}", f.to_string_lossy()))?;

      was_something_deleted = true;
    }
  }

  // Remove the partially downloaded folders
  for f in to_be_removed_folders {
    if f.try_exists().map_err(|e| {
      format!(
        "Couldn't check if the folder exists: \"{}\"\n{e}",
        f.to_string_lossy()
      )
    })? {
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
/// # Errors
///
/// If something goes wrong
pub async fn remove(upload_id: u64, game_folder: &Path) -> Result<(), String> {
  let upload_folder = get_upload_folder(game_folder, upload_id);

  // If there isn't a upload_folder, or it is empty, that means the game
  // has already been removed, so return Ok(())
  if is_folder_empty(&upload_folder)? {
    return Ok(());
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
/// # Errors
///
/// If something goes wrong
pub async fn r#move(
  upload_id: u64,
  src_game_folder: &Path,
  dst_game_folder: &Path,
) -> Result<PathBuf, String> {
  let src_upload_folder = get_upload_folder(src_game_folder, upload_id);

  // If there isn't a src_upload_folder, exit with error
  if !src_upload_folder
    .try_exists()
    .map_err(|e| format!("Couldn't check if the upload folder exists: {e}"))?
  {
    return Err("The source game folder doesn't exsit!".to_string());
  }

  let dst_upload_folder = get_upload_folder(dst_game_folder, upload_id);
  // If there is a dst_upload_folder with contents, exit with error
  if !is_folder_empty(&dst_upload_folder)? {
    return Err(format!(
      "The upload folder destination isn't empty!: \"{}\"",
      dst_upload_folder.to_string_lossy()
    ));
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
/// A `Manifest` struct with the manifest actions info, or None if the manifest isn't present
///
/// # Errors
///
/// If something goes wrong
pub async fn get_upload_manifest(
  upload_id: u64,
  game_folder: &Path,
) -> Result<Option<itch_manifest::Manifest>, String> {
  let upload_folder = get_upload_folder(game_folder, upload_id);

  itch_manifest::read_manifest(&upload_folder).await
}

/// Launchs an installed upload
///
/// # Arguments
///
/// * `upload_id` - The ID of upload which will be launched
///
/// * `game_folder` - The folder where the game uploads are placed
///
/// * `launch_method` - The launch method to use to determine the upload executable file
///
/// * `wrapper` - A list of a wrapper and its options to run the game with
///
/// * `game_arguments` - A list of arguments to launch the upload executable with
///
/// * `launch_start_callback` - A callback triggered just before the upload executable runs, providing information about what is about to be executed.
///
/// # Errors
///
/// If something goes wrong
pub async fn launch(
  upload_id: u64,
  game_folder: &Path,
  launch_method: LaunchMethod<'_>,
  wrapper: &[String],
  game_arguments: &[String],
  launch_start_callback: impl FnOnce(&Path, &tokio::process::Command),
) -> Result<(), String> {
  let upload_folder: PathBuf = get_upload_folder(game_folder, upload_id);

  // Determine the upload executable and its launch arguments from the function arguments, manifest, or heuristics.
  let (upload_executable, game_arguments): (&Path, Cow<[String]>) = match launch_method {
    // 1. If the launch method is an alternative executable, then that executable with the arguments provided to the function
    LaunchMethod::AlternativeExecutable(p) => (p, Cow::Borrowed(game_arguments)),
    // 2. If the launch method is a manifest action, use its executable
    LaunchMethod::ManifestAction(a) => {
      let ma = itch_manifest::launch_action(&upload_folder, Some(a))
        .await?
        .ok_or_else(|| format!("The provided launch action doesn't exist in the manifest: {a}"))?;
      (
        &PathBuf::from(ma.path),
        // a) If the function's game arguments are empty, use the ones from the manifest
        if game_arguments.is_empty() {
          Cow::Owned(ma.args.unwrap_or_default())
        }
        // b) Otherwise, use the provided ones
        else {
          Cow::Borrowed(game_arguments)
        },
      )
    }
    // 3. Otherwise, if the launch method are the heuristics, use them to locate the executable
    LaunchMethod::Heuristics(gp, g) => {
      // But first, check if the game has a manifest with a "play" action, and use it if possible
      let mao = itch_manifest::launch_action(&upload_folder, None).await?;

      match mao {
        // If the manifest has a "play" action, launch from it
        Some(ma) => (
          &PathBuf::from(ma.path),
          // a) If the function's game arguments are empty, use the ones from the manifest
          if game_arguments.is_empty() {
            Cow::Owned(ma.args.unwrap_or_default())
          }
          // b) Otherwise, use the provided ones
          else {
            Cow::Borrowed(game_arguments)
          },
        ),
        // Else, now use the heuristics to determine the executable, with the function's game arguments
        None => (
          &heuristics::get_game_executable(upload_folder.as_path(), gp, g).await?,
          Cow::Borrowed(game_arguments),
        ),
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
        gp.args(wrapper_iter.as_slice()).arg(&upload_executable);
        gp
      }
    }
  };

  // Add the working directory and the game arguments
  game_process
    .current_dir(&upload_folder)
    .args(game_arguments.as_ref());

  launch_start_callback(upload_executable.as_path(), &game_process);

  let mut child = game_process.spawn().map_err(|e| {
    // Error code 8: Exec format error
    if let Some(8) = e.raw_os_error() {
      "Couldn't spawn the child process because it is not an executable format for this OS\n\
          Maybe a wrapper is missing or the selected game executable isn't the correct one!"
        .to_string()
    } else {
      format!("Couldn't spawn the child process: {e}")
    }
  })?;

  child
    .wait()
    .await
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
#[must_use]
pub fn get_web_game_url(upload_id: u64) -> String {
  format!("https://html-classic.itch.zone/html/{upload_id}/index.html")
}
