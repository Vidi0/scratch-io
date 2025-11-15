use crate::errors::{
  FilesystemError, FilesystemIOErrorKind as IOErr, OtherFilesystemErrorKind as OtherErr,
};
use crate::itch_api::types::UploadID;

use std::path::{Path, PathBuf};

pub const UPLOAD_ARCHIVE_NAME: &str = "download";
pub const COVER_IMAGE_DEFAULT_FILENAME: &str = "cover.png";
pub const GAME_FOLDER: &str = "Games";

/// Convert an `&OsStr` into an `&str`
///
/// # Errors
///
/// If `os_str` doesn't contain valid unicode
pub fn os_str_as_str(os_str: &std::ffi::OsStr) -> Result<&str, FilesystemError> {
  os_str
    .to_str()
    .ok_or_else(OtherErr::InvalidUnicodeOsStr(os_str.to_owned()).attach())
}

/// Get the stem (non-extension) of the given Path
///
/// # Errors
///
/// If the path doesn't have a filename
pub fn get_file_stem(path: &Path) -> Result<&str, FilesystemError> {
  path
    .file_stem()
    .ok_or_else(OtherErr::PathWithoutFilename(path.to_owned()).attach())
    .and_then(|stem| os_str_as_str(stem))
}

/// Get the file name of the given Path
///
/// # Errors
///
/// If the path doesn't have a filename
pub fn get_file_name(path: &Path) -> Result<&str, FilesystemError> {
  path
    .file_name()
    .ok_or_else(OtherErr::PathWithoutFilename(path.to_owned()).attach())
    .and_then(|stem| os_str_as_str(stem))
}

/// Check if a path points at an existing entity
///
/// # Errors
///
/// If the filesystem operation fails
pub async fn exists(path: &Path) -> Result<bool, FilesystemError> {
  tokio::fs::try_exists(path)
    .await
    .map_err(IOErr::CouldntCheckIfExists(path.to_owned()).attach())
}

/// Get the parent of the given path
///
/// # Errors
///
/// If the path doesn't have a parent
pub async fn parent(path: &Path) -> Result<&Path, FilesystemError> {
  path
    .parent()
    .ok_or_else(OtherErr::PathWithoutParent(path.to_owned()).attach())
}

/// Return a stream over the entries within a directory
///
/// # Errors
///
/// If the filesystem operation fails
pub async fn read_dir(path: &Path) -> Result<tokio::fs::ReadDir, FilesystemError> {
  tokio::fs::read_dir(path)
    .await
    .map_err(IOErr::CouldntReadDirectory(path.to_owned()).attach())
}

/// Return a stream over the entries within a directory
///
/// # Errors
///
/// If the filesystem operation fails
pub async fn next_entry(
  read_dir: &mut tokio::fs::ReadDir,
  path: &Path,
) -> Result<Option<tokio::fs::DirEntry>, FilesystemError> {
  read_dir
    .next_entry()
    .await
    .map_err(IOErr::CouldntReadDirectoryNextEntry(path.to_owned()).attach())
}

/// Get the file type of a `DirEntry`
///
/// # Errors
///
/// If the filesystem operation fails
pub async fn file_type(
  dir_entry: &tokio::fs::DirEntry,
  path: &Path,
) -> Result<std::fs::FileType, FilesystemError> {
  dir_entry
    .file_type()
    .await
    .map_err(IOErr::CouldntGetFileType(path.to_owned()).attach())
}

/// Returns the canonical (absolute form of the path)
///
/// # Errors
///
/// If the filesystem operation fails
pub async fn get_canonical_path(path: &Path) -> Result<PathBuf, FilesystemError> {
  tokio::fs::canonicalize(path)
    .await
    .map_err(IOErr::CouldntGetCanonical(path.to_owned()).attach())
}

/// Get the `directories::BaseDirs`
///
/// # Errors
///
/// If the home directory couldn't be determined
pub fn get_basedirs() -> Result<directories::BaseDirs, FilesystemError> {
  directories::BaseDirs::new().ok_or_else(OtherErr::MissingHomeDirectory.attach())
}

/// Create a directory recursively
///
/// # Errors
///
/// If the filesystem operation fails
pub async fn create_dir(path: &Path) -> Result<(), FilesystemError> {
  tokio::fs::create_dir_all(path)
    .await
    .map_err(IOErr::CouldntCreateDirectory(path.to_owned()).attach())
}

/// Copy a file to a new path
///
/// # Errors
///
/// If the filesystem operation fails
pub async fn copy_file(from: &Path, to: &Path) -> Result<u64, FilesystemError> {
  tokio::fs::copy(from, to).await.map_err(
    IOErr::CouldntCopyFile {
      from: from.to_owned(),
      to: to.to_owned(),
    }
    .attach(),
  )
}

/// Move a file or a directory to a new path
///
/// # Errors
///
/// If the filesystem operation fails
///
/// If the new name is on a different mount point
pub async fn move_path(from: &Path, to: &Path) -> Result<(), FilesystemError> {
  tokio::fs::rename(from, to).await.map_err(
    IOErr::CouldntMove {
      from: from.to_owned(),
      to: to.to_owned(),
    }
    .attach(),
  )
}

/// Remove a file
///
/// # Errors
///
/// If the filesystem operation fails
pub async fn remove_file(path: &Path) -> Result<(), FilesystemError> {
  tokio::fs::remove_file(path)
    .await
    .map_err(IOErr::CouldntRemoveFile(path.to_owned()).attach())
}

/// Remove an empty directory
///
/// # Errors
///
/// If the filesystem operation fails
pub async fn remove_empty_dir(path: &Path) -> Result<(), FilesystemError> {
  tokio::fs::remove_dir(path)
    .await
    .map_err(IOErr::CouldntRemoveEmptyDir(path.to_owned()).attach())
}

/// Remove a directory and all its contents
///
/// # Errors
///
/// If the filesystem operation fails
pub async fn remove_dir_all(path: &Path) -> Result<(), FilesystemError> {
  tokio::fs::remove_dir_all(path)
    .await
    .map_err(IOErr::CouldntRemoveDirWithContents(path.to_owned()).attach())
}

/// Read a file or a folder's metadata
///
/// # Errors
///
/// If the filesystem operation fails
pub async fn read_metadata(path: &Path) -> Result<std::fs::Metadata, FilesystemError> {
  tokio::fs::metadata(path)
    .await
    .map_err(IOErr::CouldntReadMetadata(path.to_owned()).attach())
}

/// Checks if a given path represents a directory on the filesystem
///
/// # Errors
///
/// If the filesystem operation fails
pub async fn is_dir(path: &Path) -> Result<bool, FilesystemError> {
  read_metadata(path).await.map(|metadata| metadata.is_dir())
}

/// Set a file or a folder's permissions
///
/// # Errors
///
/// If the filesystem operation fails
pub async fn set_permissions(
  path: &Path,
  permissions: std::fs::Permissions,
) -> Result<(), FilesystemError> {
  tokio::fs::set_permissions(path, permissions)
    .await
    .map_err(IOErr::CouldntSetPermissions(path.to_owned()).attach())
}

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

/// The game folder is `dirs::home_dir`+`Games`+`game_title`
///
/// # Errors
///
/// If `dirs::home_dir` is None
pub fn get_game_folder(game_title: &str) -> Result<PathBuf, FilesystemError> {
  let mut game_folder = get_basedirs()?.home_dir().join(GAME_FOLDER);

  game_folder.push(game_title);

  Ok(game_folder)
}

/// Checks if a folder is empty
///
/// # Errors
///
/// If any filesystem operation fails
pub async fn is_folder_empty(folder: &Path) -> Result<bool, FilesystemError> {
  if folder.is_dir() {
    if next_entry(&mut read_dir(folder).await?, folder)
      .await?
      .is_none()
    {
      Ok(true)
    } else {
      Ok(false)
    }
  } else if exists(folder).await? {
    Err(FilesystemError::OtherError(
      OtherErr::ShouldBeAFolder(folder.to_owned()).into(),
    ))
  } else {
    Ok(true)
  }
}

/// Remove a folder if it is empty
///
/// Returns whether the folder was removed or not
pub async fn remove_folder_if_empty(folder: &Path) -> Result<bool, FilesystemError> {
  // Return if the folder is not empty
  let true = is_folder_empty(&folder).await? else {
    // The folder wasn't removed, so return false
    return Ok(false);
  };

  // Remove the empty folder
  remove_empty_dir(&folder).await?;

  Ok(true)
}

/// Removes a folder recursively, but checks if it is a dangerous path before doing so
pub async fn remove_folder_safely(path: &Path) -> Result<(), FilesystemError> {
  let canonical = get_canonical_path(path).await?;

  let home = get_canonical_path(get_basedirs()?.home_dir()).await?;

  if canonical == home {
    return Err(FilesystemError::OtherError(
      OtherErr::RefusingToRemoveFolder(canonical).into(),
    ));
  }

  remove_dir_all(&canonical).await
}

#[cfg_attr(not(unix), allow(unused_variables))]
pub async fn make_executable(path: &Path) -> Result<(), FilesystemError> {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;

    let metadata = read_metadata(path).await?;
    let mut permissions = metadata.permissions();
    let mode = permissions.mode();

    // If all the executable bits are already set, return Ok()
    if mode & 0o111 == 0o111 {
      return Ok(());
    }

    // Otherwise, add execute bits
    permissions.set_mode(mode | 0o111);

    set_permissions(path, permissions).await?;
  }

  Ok(())
}

/// Copy all the folder contents to another location
async fn copy_dir_all(from: PathBuf, to: PathBuf) -> Result<(), FilesystemError> {
  if !from.is_dir() {
    return Err(FilesystemError::OtherError(
      OtherErr::ShouldBeAFolder(from.to_owned()).into(),
    ));
  }

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
  if !is_dir(from).await? {
    return Err(OtherErr::ShouldBeAFolder(from.to_owned()).into());
  }

  // Create the destination parent dir
  create_dir(to).await?;

  match move_path(from, to).await {
    Ok(()) => Ok(()),
    Err(FilesystemError::IOError { error, .. })
      if error.kind() == tokio::io::ErrorKind::CrossesDevices =>
    {
      // fallback: copy + delete
      copy_dir_all(from.to_owned(), to.to_owned()).await?;
      remove_folder_safely(&from).await?;
      Ok(())
    }
    Err(e) => Err(e),
  }
}

// If path already exists, change it a bit until it doesn't. Return the available path
pub async fn find_available_path(path: &Path) -> Result<PathBuf, FilesystemError> {
  let parent = parent(path).await?;
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

  let mut child_entries = read_dir(&last_root).await?;

  // Move its children up one level
  while let Some(child) = next_entry(&mut child_entries, &last_root).await? {
    let from = child.path();
    let to = base_folder.join(child.file_name());

    if exists(&to).await? {
      // If the children filename already exists on the parent, rename it to a
      // temporal name and, at the end, rename all the temporal names in order to the final names
      let temporal_name: PathBuf = find_available_path(&to).await?;
      move_path(&from, &temporal_name).await?;

      // save the change to the collisions vector
      collisions.push((temporal_name, to));
    } else {
      move_path(&from, &to).await?;
    }
  }

  // Remove the now-empty wrapper dirs
  let mut current_root = last_root.to_owned();
  while is_folder_empty(&current_root).await? {
    let parent = parent(&current_root).await?;
    remove_empty_dir(&current_root).await?;
    current_root = parent.to_owned();
  }

  // now move all of the filenames that have collided to their original name
  for (src, dst) in &collisions {
    move_path(src, dst).await?;
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
    if next_entry(&mut entries, &last_root).await?.is_some() || first.path().is_file() {
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
