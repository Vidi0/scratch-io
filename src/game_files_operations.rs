use std::path::{Path, PathBuf};
use thiserror::Error;

pub const UPLOAD_ARCHIVE_NAME: &str = "download";
pub const COVER_IMAGE_DEFAULT_FILENAME: &str = "cover.png";
pub const GAME_FOLDER: &str = "Games";

#[derive(Error, Debug)]
pub enum FilesystemError {
  #[error("The following path doesn't have a name: \"{0}\"")]
  PathWithoutFilename(PathBuf),

  #[error("The following path doesn't have a parent: \"{0}\"")]
  PathWithoutParent(PathBuf),

  #[error("Couldn't determine the home directory")]
  MissingHomeDirectory,

  #[error("\"{0}\" is not a folder!")]
  ExpectedToBeFolderButIsNot(PathBuf),

  #[error("The folder should be empty but it isn't: \"{0}\"")]
  FolderShouldBeEmpty(PathBuf),

  #[error("Couldn't read directory elements: \"{path}\"\n{error}")]
  ReadDirectory {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't read directory next element: \"{path}\"\n{error}")]
  ReadDirectoryNextEntry {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't remove a empty folder: \"{path}\"\n{error}")]
  RemoveEmptyDir {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't remove a folder and its contents: \"{path}\"\n{error}")]
  RemoveDirWithContents {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't get the canonical (absolute) form of the path. Maybe it doesn't exist: \"{path}\"\n{error}")]
  GetCanonicalPath {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Refusing to remove folder because it is an important path!: \"{0}\"")]
  RefusingToRemoveFolder(PathBuf),

  #[error("Couldn't read the file/directory metadata of: \"{path}\"\n{error}")]
  ReadMetadata {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't change the file/directory permissions of: \"{path}\"\n{error}")]
  ChangePermissions {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't create the folder: \"{path}\"\n{error}")]
  CreateFolder {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't get the file type (file or folder) of: \"{path}\"\n{error}")]
  GetFileType {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't copy file:\n  Source: \"{src}\"\n  Destination: \"{dst}\"\n{error}")]
  CopyFile {
    #[source]
    error: tokio::io::Error,
    src: PathBuf,
    dst: PathBuf,
  },

  #[error("Couldn't remove file:\"{path}\"\n {error}")]
  RemoveFile {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't move file/directory:\n  Source: \"{src}\"\n  Destination: \"{dst}\"\n{error}")]
  Move {
    #[source]
    error: tokio::io::Error,
    src: PathBuf,
    dst: PathBuf,
  },

  #[error("Couldn't check if the path exists: \"{path}\"\n{error}")]
  CheckIfPathExists {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't open the file: \"{path}\"\n{error}")]
  OpenFile {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },  
}

/// Get the upload folder based on its game folder
pub fn get_upload_folder(game_folder: impl AsRef<Path>, upload_id: u64) -> PathBuf {
  game_folder.as_ref().join(format!("{upload_id}"))
}

/// Get the upload archive path based on its game folder and upload_id
pub fn get_upload_archive_path(game_folder: impl AsRef<Path>, upload_id: u64, upload_filename: &str) -> PathBuf {
  game_folder.as_ref().join(format!("{upload_id}-{UPLOAD_ARCHIVE_NAME}-{upload_filename}"))
}

/// Adds a .part extension to the given Path
pub fn add_part_extension(file: impl AsRef<Path>) -> Result<PathBuf, FilesystemError> {
  let filename = file.as_ref()
    .file_name()
    .ok_or_else(|| FilesystemError::PathWithoutFilename(file.as_ref().to_path_buf()))?
    .to_string_lossy();

  Ok(file.as_ref().with_file_name(format!("{filename}.part")))
}

/// The game folder is `dirs::home_dir`+`Games`+`game_title`
/// 
/// It fais if dirs::home_dir is None
pub fn get_game_folder(game_title: &str) -> Result<PathBuf, FilesystemError> {
  let mut game_folder = directories::BaseDirs::new()
    .ok_or(FilesystemError::MissingHomeDirectory)?
    .home_dir()
    .join(GAME_FOLDER);

  game_folder.push(game_title);

  Ok(game_folder)
}

/// Gets the stem (non-extension) of the given Path
pub fn get_file_stem(path: impl AsRef<Path>) -> Result<String, FilesystemError> {
  path.as_ref()
    .file_stem()
    .ok_or_else(|| FilesystemError::PathWithoutFilename(path.as_ref().to_path_buf()))
    .map(|stem| stem.to_string_lossy().to_string())
}

/// Checks if a folder is empty
pub fn is_folder_empty(folder: impl AsRef<Path>) -> Result<bool, FilesystemError> {
  if folder.as_ref().is_dir() {
    if folder.as_ref().read_dir().map_err(|error| FilesystemError::ReadDirectory { error, path: folder.as_ref().to_path_buf() })?.next().is_none() {
      Ok(true)
    } else {
      Ok(false)
    }
  } else if folder.as_ref().exists() {
    Err(FilesystemError::ExpectedToBeFolderButIsNot(folder.as_ref().to_path_buf()))
  } else {
    Ok(true)
  }
}

/// Remove a folder if it is empty
/// 
/// Returns whether the folder was removed or not
pub async fn remove_folder_if_empty(folder: impl AsRef<Path>) -> Result<bool, FilesystemError> {
  // Return if the folder is not empty
  let true = is_folder_empty(&folder)? else {
    // The folder wasn't removed, so return false
    return Ok(false);
  };

  // Remove the empty folder
  tokio::fs::remove_dir(&folder).await
    .map_err(|error| FilesystemError::RemoveEmptyDir { error, path: folder.as_ref().to_path_buf() })?;

  Ok(true)
}

/// Removes a folder recursively, but checks if it is a dangerous path before doing so
pub async fn remove_folder_safely(path: impl AsRef<Path>) -> Result<(), FilesystemError> {
  let canonical = tokio::fs::canonicalize(&path).await
    .map_err(|error| FilesystemError::GetCanonicalPath { error, path: path.as_ref().to_path_buf() })?;

  let home = {
    let basedirs = directories::BaseDirs::new()
      .ok_or(FilesystemError::MissingHomeDirectory)?;
    
    basedirs.home_dir()
      .canonicalize()
      .map_err(|error| FilesystemError::GetCanonicalPath { error, path: basedirs.home_dir().to_path_buf() })?
  };

  if canonical == home {
    return Err(FilesystemError::RefusingToRemoveFolder(canonical));
  }

  tokio::fs::remove_dir_all(&canonical).await
    .map_err(|error| FilesystemError::RemoveDirWithContents { error, path: canonical })?;

  Ok(())
}

#[cfg_attr(not(unix), allow(unused_variables))]
pub fn make_executable(path: impl AsRef<Path>) -> Result<(), FilesystemError> {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;

    let metadata = std::fs::metadata(&path)
      .map_err(|error| FilesystemError::ReadMetadata { error, path: path.as_ref().to_path_buf() })?;
    let mut permissions = metadata.permissions();
    let mode = permissions.mode();
    
    // If all the executable bits are already set, return Ok()
    if mode & 0o111 == 0o111 {
      return Ok(());
    }

    // Otherwise, add execute bits
    permissions.set_mode(mode | 0o111);

    std::fs::set_permissions(&path, permissions)
      .map_err(|error| FilesystemError::ChangePermissions { error, path: path.as_ref().to_path_buf() })?;
  }

  Ok(())
}

/// Copy all the folder contents to another location
async fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), FilesystemError> {
  if !src.as_ref().is_dir() {
    return Err(FilesystemError::ExpectedToBeFolderButIsNot(src.as_ref().to_path_buf()));
  }

  let mut queue: std::collections::VecDeque<(PathBuf, PathBuf)> = std::collections::VecDeque::new();
  queue.push_back((src.as_ref().to_path_buf(), dst.as_ref().to_path_buf()));

  while let Some((src, dst)) = queue.pop_front() {
    tokio::fs::create_dir_all(&dst).await
      .map_err(|error| FilesystemError::CreateFolder { error, path: dst.to_path_buf() })?;

    let mut entries = tokio::fs::read_dir(&src).await
      .map_err(|error| FilesystemError::ReadDirectory { error, path: src.to_path_buf() })?;

    while let Some(entry) = entries.next_entry().await.map_err(|error| FilesystemError::ReadDirectoryNextEntry { error, path: src.to_path_buf() })? {
      let src_path = entry.path();
      let dst_path = dst.join(entry.file_name());

      if entry.file_type().await.map_err(|error| FilesystemError::GetFileType { error, path: src_path.to_path_buf() })?.is_dir() {
        queue.push_back((src_path, dst_path));
      } else {
        tokio::fs::copy(&src_path, &dst_path)
          .await
          .map_err(|error| FilesystemError::CopyFile { error, src: src_path, dst: dst_path })?;
      } 
    }
  }

  Ok(())
}

/// Move a folder and its contents to another location
/// 
/// It also works if the destination is on another filesystem
pub async fn move_folder(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), FilesystemError> {
  if !src.as_ref().is_dir() {
    return Err(FilesystemError::ExpectedToBeFolderButIsNot(src.as_ref().to_path_buf()));
  }

  // Create the destination parent dir
  tokio::fs::create_dir_all(&dst).await
    .map_err(|error| FilesystemError::CreateFolder { error, path: dst.as_ref().to_path_buf() })?;

  match tokio::fs::rename(&src, &dst).await {
    Ok(_) => Ok(()),
    Err(e) if e.kind() == tokio::io::ErrorKind::CrossesDevices => {
      // fallback: copy + delete
      copy_dir_all(&src, &dst).await?;
      remove_folder_safely(&src).await
    }
    Err(error) => Err(FilesystemError::Move { error, src: src.as_ref().to_path_buf(), dst: dst.as_ref().to_path_buf() }),
  }
}

// If path already exists, change it a bit until it doesn't. Return the available path
pub fn find_available_path(path: impl AsRef<Path>) -> Result<PathBuf, FilesystemError> {
  let parent = path.as_ref().parent()
    .ok_or_else(|| FilesystemError::PathWithoutParent(path.as_ref().to_path_buf()))?;

  let mut i = 0;
  loop {
    // i is printed in hexadecimal because it looks better
    let current_filename = format!("{}{:x}",
      path.as_ref().file_name()
        .ok_or_else(|| FilesystemError::PathWithoutFilename(path.as_ref().to_path_buf()))?
        .to_string_lossy(),
      i
    );
    let current_path: PathBuf = parent.join(current_filename);

    if !current_path.exists() {
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
async fn move_folder_child(last_root: impl AsRef<Path>, base_folder: impl AsRef<Path>) -> Result<(), FilesystemError> {
  // If a file or a folder already exists in the destination folder, rename it and save the new name and
  // the original name to this Vector. At the end, after removing the parent folder, rename all elements of this Vector
  let mut collisions: Vec<(PathBuf, PathBuf)> = Vec::new();

  let mut child_entries = tokio::fs::read_dir(&last_root).await
    .map_err(|error| FilesystemError::ReadDirectory { error, path: last_root.as_ref().to_path_buf() })?;

  // Move its children up one level
  while let Some(child) = child_entries.next_entry().await.map_err(|error| FilesystemError::ReadDirectoryNextEntry { error, path: last_root.as_ref().to_path_buf() })? {

    let from = child.path();
    let to = base_folder.as_ref().join(child.file_name());

    if !to.try_exists().map_err(|error| FilesystemError::CheckIfPathExists { error, path: to.to_path_buf() })? {
      tokio::fs::rename(&from, &to).await
        .map_err(|error| FilesystemError::Move { error, src: from, dst: to })?;
    } else {
      // If the children filename already exists on the parent, rename it to a
      // temporal name and, at the end, rename all the temporal names in order to the final names
      let temporal_name: PathBuf = find_available_path(&to)?;
      tokio::fs::rename(&from, &temporal_name).await
        .map_err(|error| FilesystemError::Move { error, src: from, dst: temporal_name.to_path_buf() })?;

      // save the change to the collisions vector
      collisions.push((temporal_name, to));
    }
  }

  // Remove the now-empty wrapper dirs
  let mut current_root = last_root.as_ref().to_path_buf();
  while is_folder_empty(&current_root)? {
    let parent = current_root.parent()
      .ok_or_else(|| FilesystemError::PathWithoutParent(current_root.to_path_buf()))?
      .to_path_buf();

    tokio::fs::remove_dir(&current_root).await
      .map_err(|error| FilesystemError::RemoveEmptyDir { error, path: current_root })?;

    current_root = parent;
  }

  // now move all of the filenames that have collided to their original name
  for (src, dst) in collisions {
    tokio::fs::rename(&src, &dst).await
      .map_err(|error| FilesystemError::Move { error, src, dst })?;
  }

  Ok(())
}

/// This fuction removes all the common root folders that only contain another folder
/// and unwraps its children to its parent
/// 
/// If applied to the folder `foo` in `/foo/bar/baz.txt`, the remainig structure is `/foo/baz.txt`
pub async fn remove_root_folder(folder: impl AsRef<Path>) -> Result<(), FilesystemError> {
  // This variable is the last nested root of the folder
  let mut last_root: PathBuf = folder.as_ref().to_path_buf();
  let mut is_there_any_root: bool = false;

  loop {
    // List entries
    let mut entries: tokio::fs::ReadDir = tokio::fs::read_dir(&last_root).await
      .map_err(|error| FilesystemError::ReadDirectory { error, path: last_root.to_path_buf() })?;

    // First entry (or empty)
    let Some(first) = entries.next_entry().await.map_err(|error| FilesystemError::ReadDirectoryNextEntry { error, path: last_root.to_path_buf() })? else {
      break;
    };

    // If there’s another entry, stop (not a single root)
    // If the entry is a file, also stop
    if entries.next_entry().await.map_err(|error| FilesystemError::ReadDirectoryNextEntry { error, path: last_root.to_path_buf() })?.is_some() || first.path().is_file() {
      break;
    };

    // At this point, we know that first is a wrapper dir,
    // so set last_root to that and loop again in case there are nested roots
    is_there_any_root = true;
    last_root = first.path();
  }

  // Remove the wrappers
  if is_there_any_root {
    move_folder_child(last_root, folder).await?;
  }
  
  Ok(())
}
