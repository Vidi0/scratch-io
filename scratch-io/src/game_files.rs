use crate::errors::{FilesystemError, OtherFilesystemErrorKind as OtherErr};
use crate::filesystem::*;
use crate::itch_api::types::UploadID;

use std::path::{Path, PathBuf};

pub const UPLOAD_ARCHIVE_NAME: &str = "download";
pub const COVER_IMAGE_DEFAULT_FILENAME: &str = "cover.png";

/// Get the upload folder based on its game folder
pub fn get_upload_folder(game_folder: &Path, upload_id: UploadID) -> PathBuf {
  game_folder.join(format!("{upload_id}"))
}

/// Get the upload archive path based on its game folder and `upload_id`
pub fn get_upload_archive_path(
  game_folder: &Path,
  upload_id: UploadID,
  upload_filename: &str,
) -> PathBuf {
  game_folder.join(format!(
    "{upload_id}-{UPLOAD_ARCHIVE_NAME}-{upload_filename}"
  ))
}

/// Adds a .part extension to the given Path
pub fn add_part_extension(file: &Path) -> Result<PathBuf, FilesystemError> {
  let filename = get_file_name(file)?;
  Ok(file.with_file_name(format!("{filename}.part")))
}

/// Remove a folder if it is empty
///
/// Returns whether the folder was removed or not
pub async fn remove_folder_if_empty(folder: &Path) -> Result<bool, FilesystemError> {
  // Return if the folder is not empty
  let true = is_folder_empty(folder).await? else {
    // The folder wasn't removed, so return false
    return Ok(false);
  };

  // Remove the empty folder
  remove_empty_dir(folder).await?;

  Ok(true)
}

/// Removes a folder recursively, but checks if it is a dangerous path before doing so
pub async fn remove_folder_safely(path: &Path) -> Result<(), FilesystemError> {
  let canonical = get_canonical_path(path).await?;

  if let Some(home) = std::env::home_dir()
    && canonical == get_canonical_path(&home).await?
  {
    return Err(OtherErr::RefusingToRemoveFolder(canonical).into());
  }

  remove_dir_all(&canonical).await
}

/// Copy all the folder contents to another location
async fn copy_dir_all(from: PathBuf, to: PathBuf) -> Result<(), FilesystemError> {
  ensure_is_dir(&from).await?;
  create_dir(&to).await?;

  let mut queue: std::collections::VecDeque<(PathBuf, PathBuf)> = std::collections::VecDeque::new();
  queue.push_back((from, to));

  while let Some((from, to)) = queue.pop_front() {
    let mut entries = read_dir(&from).await?;

    while let Some(entry) = next_entry(&mut entries, &from).await? {
      let from_path = entry.path();
      let to_path = to.join(entry.file_name());

      if file_type(&entry, &from).await?.is_dir() {
        create_dir(&to).await?;
        queue.push_back((from_path, to_path));
      } else {
        copy_file(&from_path, &to_path).await?;
      }
    }
  }

  Ok(())
}

/// Move a folder and its contents to another location
///
/// It also works if the destination is on another filesystem
pub async fn move_folder(from: &Path, to: &Path) -> Result<(), FilesystemError> {
  ensure_is_dir(from).await?;

  // Create the destination parent dir
  create_dir(to).await?;

  match rename(from, to).await {
    Ok(()) => Ok(()),
    Err(FilesystemError::IOError { error, .. })
      if error.kind() == tokio::io::ErrorKind::CrossesDevices =>
    {
      // fallback: copy + delete
      copy_dir_all(from.to_owned(), to.to_owned()).await?;
      remove_folder_safely(from).await?;
      Ok(())
    }
    Err(e) => Err(e),
  }
}

// If path already exists, change it a bit until it doesn't. Return the available path
pub async fn find_available_path(path: &Path) -> Result<PathBuf, FilesystemError> {
  let parent = parent(path)?;
  let filename = get_file_name(path)?;

  let mut i = 0;
  loop {
    // i is printed in hexadecimal because it looks better
    let current_filename = format!("{filename}{i:x}");
    let current_path: PathBuf = parent.join(current_filename);

    if !exists(&current_path).await? {
      return Ok(current_path);
    }
    i += 1;
  }
}

/// This function takes `base_folder` and `last_root`, which has to be a child or grandchild of `base_folder`
///
/// Then, it moves all `last_root` contents into `base_folder`,
/// and removes all the empty folders between `last_root` and `base_folder`
///
/// If applied to the folder `foo/` and `foo/bar/` in `/foo/bar/baz.txt`, the remainig structure is `/foo/baz.txt`
async fn move_folder_child(last_root: &Path, base_folder: &Path) -> Result<(), FilesystemError> {
  // If a file or a folder already exists in the destination folder, rename it and save the new name and
  // the original name to this Vector. At the end, after removing the parent folder, rename all elements of this Vector
  let mut collisions: Vec<(PathBuf, PathBuf)> = Vec::new();

  let mut child_entries = read_dir(last_root).await?;

  // Move its children up one level
  while let Some(child) = next_entry(&mut child_entries, last_root).await? {
    let from = child.path();
    let to = base_folder.join(child.file_name());

    if exists(&to).await? {
      // If the children filename already exists on the parent, rename it to a
      // temporal name and, at the end, rename all the temporal names in order to the final names
      let temporal_name: PathBuf = find_available_path(&to).await?;
      rename(&from, &temporal_name).await?;

      // save the change to the collisions vector
      collisions.push((temporal_name, to));
    } else {
      rename(&from, &to).await?;
    }
  }

  // Remove the now-empty wrapper dirs
  let mut current_root = last_root.to_owned();
  while is_folder_empty(&current_root).await? {
    let parent = parent(&current_root)?;
    remove_empty_dir(&current_root).await?;
    current_root = parent.to_owned();
  }

  // now move all of the filenames that have collided to their original name
  for (src, dst) in &collisions {
    rename(src, dst).await?;
  }

  Ok(())
}

/// This fuction removes all the common root folders that only contain another folder
/// and unwraps its children to its parent
///
/// If applied to the folder `foo` in `/foo/bar/baz.txt`, the remainig structure is `/foo/baz.txt`
pub async fn remove_root_folder(folder: &Path) -> Result<(), FilesystemError> {
  // This variable is the last nested root of the folder
  let mut last_root: PathBuf = folder.to_path_buf();
  let mut is_there_any_root: bool = false;

  loop {
    // List entries
    let mut entries = read_dir(&last_root).await?;

    // First entry (or empty)
    let Some(first) = next_entry(&mut entries, &last_root).await? else {
      break;
    };

    // If there’s another entry, stop (not a single root)
    // If the entry is a file, also stop
    if next_entry(&mut entries, &last_root).await?.is_some()
      || file_type(&first, &last_root).await?.is_file()
    {
      break;
    }

    // At this point, we know that first is a wrapper dir,
    // so set last_root to that and loop again in case there are nested roots
    is_there_any_root = true;
    last_root = first.path();
  }

  // Remove the wrappers
  if is_there_any_root {
    move_folder_child(&last_root, folder).await?;
  }

  Ok(())
}
