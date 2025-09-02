use std::path::{Path, PathBuf};

pub fn find_cover_filename<P: AsRef<Path>>(game_folder: P) -> Result<Option<String>, String> { 
  let child_entries = std::fs::read_dir(&game_folder)
    .map_err(|e| format!("Couldn't read direcotory: \"{}\"\n{e}", game_folder.as_ref().to_string_lossy()))?;

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

pub fn make_executable<P: AsRef<Path>>(path: P) -> Result<(), String> {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;

    let metadata = std::fs::metadata(&path)
      .map_err(|e| format!("Couldn't read file metadata of \"{}\": {e}", path.as_ref().to_string_lossy()))?;
    let mut permissions = metadata.permissions();
    
    let mode = permissions.mode();
    permissions.set_mode(mode | 0o111);

    std::fs::set_permissions(&path, permissions)
      .map_err(|e| format!("Couldn't set permissions of \"{}\": {e}", path.as_ref().to_string_lossy()))?;
  }

  Ok(())
}

/// Joins the upload folder and the upload id
pub fn get_upload_folder<P: AsRef<Path>>(game_folder: P, upload_id: u64) -> PathBuf {
  game_folder.as_ref().join(upload_id.to_string())
}

/// The game folder is `dirs::home_dir`+`Games`+`game_title`
/// 
/// It fais if dirs::home_dir is None
pub fn get_game_folder(game_title: &str) -> Result<PathBuf, String> {
  dirs::home_dir()
    .ok_or(format!("Couldn't determine the home directory"))
    .map(|p| 
      p.join("Games")
        .join(game_title)
    )
}

/// Removes a folder recursively, but checks if it is a dangerous path before doing so
pub async fn remove_folder_safely<P: AsRef<Path>>(path: P) -> Result<(), String> {
  let canonical = tokio::fs::canonicalize(&path).await
    .map_err(|e| format!("Error getting the canonical form of the path!: {e}"))?;

  let home = dirs::home_dir()
    .ok_or(format!("Couldn't determine the home directory"))?
    .canonicalize()
    .map_err(|e| format!("Error getting the canonical form of the path!: {e}"))?;

  if canonical == home {
    Err(format!("Refusing to remove home directory!"))?
  }

  tokio::fs::remove_dir_all(&path).await
    .map_err(|e| format!("Couldn't remove directory: \"{}\"\n{e}", path.as_ref().to_string_lossy()))?;

  Ok(())
}

/// Checks if a folder is empty
pub fn is_folder_empty<P: AsRef<Path>>(folder: P) -> Result<bool, String> {
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
          .map_err(|e| format!("Couldn't copy file:\n  from: \"{}\"\n  to: \"{}\"\n{e}", src_path.to_string_lossy(), dst_path.to_string_lossy()))?;
      } 
    }
  }

  Ok(())
}

/// This function will remove a folder AND ITS CONTENTS if it doesn't have another folder inside
pub async fn remove_folder_without_child_folders<P: AsRef<Path>>(folder: P) -> Result<(), String> {
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
pub async fn move_folder<P: AsRef<Path>>(src: P, dst: P) -> Result<(), String> {
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
    Err(e) => Err(format!("Couldn't move the folder:\n  from: \"{}\"\n  to: \"{}\"\n{e}", src.as_ref().to_string_lossy(), dst.as_ref().to_string_lossy())),
  }
}

