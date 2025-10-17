use std::path::{Path, PathBuf};

pub const UPLOAD_ARCHIVE_NAME: &str = "download";
pub const COVER_IMAGE_DEFAULT_FILENAME: &str = "cover.png";
pub const GAME_FOLDER: &str = "Games";

/// Get the upload folder based on its game folder
pub fn get_upload_folder(game_folder: impl AsRef<Path>, upload_id: u64) -> PathBuf {
  game_folder.as_ref().join(format!("{upload_id}"))
}

/// Get the upload archive path based on its game folder and upload_id
pub fn get_upload_archive_path(game_folder: impl AsRef<Path>, upload_id: u64, upload_filename: &str) -> PathBuf {
  game_folder.as_ref().join(format!("{upload_id}-{UPLOAD_ARCHIVE_NAME}-{upload_filename}"))
}

/// Adds a .part extension to the given Path
pub fn add_part_extension(file: impl AsRef<Path>) -> Result<PathBuf, String> {
  let filename = file.as_ref().file_name()
    .ok_or_else(|| format!("Couldn't add .part extension to the file because it doesn't have a name!: {}", file.as_ref().to_string_lossy()))?
    .to_string_lossy()
    .to_string();

  Ok(file.as_ref().with_file_name(format!("{filename}.part")))
}

/// The game folder is `dirs::home_dir`+`Games`+`game_title`
/// 
/// It fais if dirs::home_dir is None
pub fn get_game_folder(game_title: &str) -> Result<PathBuf, String> {
  let mut game_folder = directories::BaseDirs::new()
    .ok_or_else(|| "Couldn't determine the home directory".to_string())?
    .home_dir()
    .join(GAME_FOLDER);

  game_folder.push(game_title);

  Ok(game_folder)
}

/// Gets the stem (non-extension) of the given Path
pub fn get_file_stem(path: impl AsRef<Path>) -> Result<String, String> {
  path.as_ref()
    .file_stem()
    .ok_or_else(|| format!("Error removing stem from path: \"{}\"", path.as_ref().to_string_lossy()))
    .map(|stem| stem.to_string_lossy().to_string())
}

/// Checks if a folder is empty
pub fn is_folder_empty(folder: impl AsRef<Path>) -> Result<bool, String> {
  if folder.as_ref().is_dir() {
    if folder.as_ref().read_dir().map_err(|e| e.to_string())?.next().is_none() {
      Ok(true)
    } else {
      Ok(false)
    }
  } else if folder.as_ref().exists() {
    Err(format!("Error while cheching if folder is empty: \"{}\" is not a folder!", folder.as_ref().to_string_lossy()))
  } else {
    Ok(true)
  }
}

/// Remove a folder if it is empty
/// 
/// Returns whether the folder was removed or not
pub async fn remove_folder_if_empty(folder: impl AsRef<Path>) -> Result<bool, String> {
  // Return if the folder is not empty
  let true = is_folder_empty(&folder)? else {
    // The folder wasn't removed, so return false
    return Ok(false);
  };

  // Remove the empty folder
  tokio::fs::remove_dir(&folder).await
    .map_err(|e| format!("Couldn't remove empty folder: \"{}\"\n{e}", folder.as_ref().to_string_lossy()))?;

  Ok(true)
}

/// Removes a folder recursively, but checks if it is a dangerous path before doing so
pub async fn remove_folder_safely(path: impl AsRef<Path>) -> Result<(), String> {
  let canonical = tokio::fs::canonicalize(&path).await
    .map_err(|e| format!("Error getting the canonical form of the game folder! Maybe it doesn't exist: {}\n{e}", path.as_ref().to_string_lossy()))?;

  let home = directories::BaseDirs::new()
    .ok_or_else(|| "Couldn't determine the home directory".to_string())?
    .home_dir()
    .canonicalize()
    .map_err(|e| format!("Error getting the canonical form of the system home folder! Why?\n{e}"))?;

  if canonical == home {
    Err("Refusing to remove home directory!".to_string())?
  }

  tokio::fs::remove_dir_all(&path).await
    .map_err(|e| format!("Couldn't remove directory: \"{}\"\n{e}", path.as_ref().to_string_lossy()))?;

  Ok(())
}

#[cfg_attr(not(unix), allow(unused_variables))]
pub fn make_executable(path: impl AsRef<Path>) -> Result<(), String> {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;

    let metadata = std::fs::metadata(&path)
      .map_err(|e| format!("Couldn't read file metadata of \"{}\": {e}", path.as_ref().to_string_lossy()))?;
    let mut permissions = metadata.permissions();
    let mode = permissions.mode();
    
    // If all the executable bits are already set, return Ok()
    if mode & 0o111 == 0o111 {
      return Ok(());
    }

    // Otherwise, add execute bits
    permissions.set_mode(mode | 0o111);

    std::fs::set_permissions(&path, permissions)
      .map_err(|e| format!("Couldn't set permissions of \"{}\": {e}", path.as_ref().to_string_lossy()))?;
  }

  Ok(())
}

/// Copy all the folder contents to another location
async fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), String> {
  if !src.as_ref().is_dir() {
    return Err(format!("Not a folder: \"{}\"", src.as_ref().to_string_lossy()));
  }

  let mut queue: std::collections::VecDeque<(PathBuf, PathBuf)> = std::collections::VecDeque::new();
  queue.push_back((src.as_ref().to_path_buf(), dst.as_ref().to_path_buf()));

  while let Some((src, dst)) = queue.pop_front() {
    tokio::fs::create_dir_all(&dst).await
      .map_err(|e| format!("Couldn't create folder \"{}\": {e}", dst.as_path().to_string_lossy()))?;

    let mut entries = tokio::fs::read_dir(&src).await
      .map_err(|e| format!("Couldn't read dir \"{}\": {e}", src.as_path().to_string_lossy()))?;

    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
      let src_path = entry.path();
      let dst_path = dst.join(entry.file_name());

      if entry.file_type().await.map_err(|e| e.to_string())?.is_dir() {
        queue.push_back((src_path, dst_path));
      } else {
        tokio::fs::copy(&src_path, &dst_path)
          .await
          .map_err(|e| format!("Couldn't copy file:\n  Source: \"{}\"\n  Destination: \"{}\"\n{e}", src_path.to_string_lossy(), dst_path.to_string_lossy()))?;
      } 
    }
  }

  Ok(())
}

/// Move a folder and its contents to another location
/// 
/// It also works if the destination is on another filesystem
pub async fn move_folder(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), String> {
  if !src.as_ref().is_dir() {
    Err(format!("The source folder doesn't exist!: \"{}\"", src.as_ref().to_string_lossy()))?;
  }

  // Create the destination parent dir
  tokio::fs::create_dir_all(&dst).await
    .map_err(|e| format!("Couldn't create folder: \"{}\"\n{e}", dst.as_ref().to_string_lossy()))?;

  match tokio::fs::rename(&src, &dst).await {
    Ok(_) => Ok(()),
    Err(e) if e.kind() == tokio::io::ErrorKind::CrossesDevices => {
      // fallback: copy + delete
      copy_dir_all(&src, &dst).await?;
      remove_folder_safely(&src).await?;
      Ok(())
    }
    Err(e) => Err(format!("Couldn't move the folder:\n  Source: \"{}\"\n  Destination: \"{}\"\n{e}", src.as_ref().to_string_lossy(), dst.as_ref().to_string_lossy())),
  }
}

// If path already exists, change it a bit until it doesn't. Return the available path
pub fn find_available_path(path: impl AsRef<Path>) -> Result<PathBuf, String> {
  let parent = path.as_ref().parent()
    .ok_or_else(|| format!("Error getting parent of: \"{}\"", path.as_ref().to_string_lossy()))?;

  let mut i = 0;
  loop {
    // i is printed in hexadecimal because it looks better
    let current_filename = format!("{}{:x}",
      path.as_ref().file_name()
        .ok_or_else(|| format!("Error getting file name of: \"{}\"", path.as_ref().to_string_lossy()))?
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
async fn move_folder_child(last_root: impl AsRef<Path>, base_folder: impl AsRef<Path>) -> Result<(), String> {
  // If a file or a folder already exists in the destination folder, rename it and save the new name and
  // the original name to this Vector. At the end, after removing the parent folder, rename all elements of this Vector
  let mut collisions: Vec<(PathBuf, PathBuf)> = Vec::new();

  let mut child_entries = tokio::fs::read_dir(&last_root).await
    .map_err(|e| format!("Couldn't read folder entries of: \"{}\"\n{e}", last_root.as_ref().to_string_lossy()))?;

  // Move its children up one level
  while let Some(child) = child_entries.next_entry().await.map_err(|e| format!("Couldn't get next folder entry: \"{}\"\n{e}", last_root.as_ref().to_string_lossy()))? {

    let from = child.path();
    let to = base_folder.as_ref().join(child.file_name());

    if !to.try_exists().map_err(|e| format!("Couldn't check is the path exists!: \"{}\"\n{e}", to.to_string_lossy()))? {
      tokio::fs::rename(&from, &to).await
        .map_err(|e| format!("Couldn't move the item:\n  Source: \"{}\"\n  Destination: \"{}\"\n{e}", from.to_string_lossy(), to.to_string_lossy()))?;
    } else {
      // If the children filename already exists on the parent, rename it to a
      // temporal name and, at the end, rename all the temporal names in order to the final names
      let temporal_name: PathBuf = find_available_path(&to)?;
      tokio::fs::rename(&from, &temporal_name).await
        .map_err(|e| format!("Couldn't move the item:\n  Source: \"{}\"\n  Destination: \"{}\"\n{e}", from.to_string_lossy(), temporal_name.to_string_lossy()))?;

      // save the change to the collisions vector
      collisions.push((temporal_name, to));
    }
  }

  // Remove the now-empty wrapper dirs
  let mut current_root = last_root.as_ref().to_path_buf();
  while is_folder_empty(&current_root)? {
    let parent = current_root.parent()
      .ok_or_else(|| format!("Error getting parent of: \"{}\"", current_root.to_string_lossy()))?
      .to_path_buf();

    tokio::fs::remove_dir(&current_root).await
      .map_err(|e| format!("Couldn't remove empty folder: \"{}\"\n{e}", current_root.to_string_lossy()))?;

    current_root = parent;
  }

  // now move all of the filenames that have collided to their original name
  for (src, dst) in collisions.iter() {
    tokio::fs::rename(&src, &dst).await
      .map_err(|e| format!("Couldn't move the item:\n  Source: \"{}\"\n  Destination: \"{}\"\n{e}", src.to_string_lossy(), dst.to_string_lossy()))?;
  }

  Ok(())
}

/// This fuction removes all the common root folders that only contain another folder
/// and unwraps its children to its parent
/// 
/// If applied to the folder `foo` in `/foo/bar/baz.txt`, the remainig structure is `/foo/baz.txt`
pub async fn remove_root_folder(folder: impl AsRef<Path>) -> Result<(), String> {
  // This variable is the last nested root of the folder
  let mut last_root: PathBuf = folder.as_ref().to_path_buf();
  let mut is_there_any_root: bool = false;

  loop {
    // List entries
    let mut entries: tokio::fs::ReadDir = tokio::fs::read_dir(&last_root).await
      .map_err(|e| format!("Couldn't read folder entries of: \"{}\"\n{e}", last_root.to_string_lossy()))?;

    // First entry (or empty)
    let Some(first) = entries.next_entry().await.map_err(|e| format!("Couldn't get next folder entry: \"{}\"\n{e}", last_root.to_string_lossy()))? else {
      break;
    };

    // If there’s another entry, stop (not a single root)
    // If the entry is a file, also stop
    if entries.next_entry().await.map_err(|e| format!("Couldn't get next folder entry: \"{}\"\n{e}", last_root.to_string_lossy()))?.is_some() || first.path().is_file() {
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
