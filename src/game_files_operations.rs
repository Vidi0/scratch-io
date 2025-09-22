use std::{ffi::OsStr, path::{Path, PathBuf}};

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

#[cfg_attr(not(unix), allow(unused_variables))]
pub fn make_executable<P: AsRef<Path>>(path: P) -> Result<(), String> {
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

/// Adds a .part extension to the given Path
pub fn add_part_extension<P: AsRef<Path>>(file: P) -> Result<PathBuf, String> {
  Ok(
    file.as_ref().with_file_name(format!(
      "{}.part",
      file.as_ref().file_name()
        .ok_or(format!("Couldn't add .part extension to the file because it doesn't have a name!: {}", file.as_ref().to_string_lossy()))?
        .to_string_lossy()
    ))
  )
}

#[allow(dead_code)]
pub async fn move_upload_folder_to_backup<P: AsRef<Path>>(game_folder: P, upload_id: u64) -> Result<PathBuf, String> {
  let upload_folder = get_upload_folder(&game_folder, upload_id);
  let new_folder = find_available_path(game_folder.as_ref().join(format!("{upload_id}.old")))?;

  move_folder(&upload_folder, &new_folder).await?;

  Ok(new_folder)
}

/// Removes a folder recursively, but checks if it is a dangerous path before doing so
pub async fn remove_folder_safely<P: AsRef<Path>>(path: P) -> Result<(), String> {
  let canonical = tokio::fs::canonicalize(&path).await
    .map_err(|e| format!("Error getting the canonical form of the game folder! Maybe it doesn't exist: {}\n{e}", path.as_ref().to_string_lossy()))?;

  let home = dirs::home_dir()
    .ok_or(format!("Couldn't determine the home directory"))?
    .canonicalize()
    .map_err(|e| format!("Error getting the canonical form of the system home folder! Why?\n{e}"))?;

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
    if folder.as_ref().read_dir().map_err(|e| e.to_string())?.next().is_none() {
      Ok(true)
    } else {
      Ok(false)
    }
  } else {
    if folder.as_ref().exists() {
      Err(format!("Error while cheching if folder is empty: \"{}\" is not a folder!", folder.as_ref().to_string_lossy()))
    } else {
      Ok(true)
    }
  }
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

/// Returns an error if the provided folder has any child other than `only_child_name`
pub async fn ensure_folder_has_only_child<P: AsRef<Path>>(folder: P, only_child_name: &OsStr) -> Result<(), String> {
  let mut entries = tokio::fs::read_dir(&folder).await
    .map_err(|e| format!("Couldn't read dir \"{}\": {e}", folder.as_ref().to_string_lossy()))?;

  // Return an error if the first entry isn't the one allowed
  if let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
    if entry.file_name() != only_child_name {
      return Err(format!("The folder: \"{}\" should only have one child, \"{}\", but it has one child named: \"{}\"", folder.as_ref().to_string_lossy(), only_child_name.to_string_lossy(), entry.file_name().to_string_lossy()));
    }
  }

  // Return an error if the folder has more than one entry
  if let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
    return Err(format!("The folder: \"{}\" should only have one child, \"{}\", but it also one child named: \"{}\"", folder.as_ref().to_string_lossy(), only_child_name.to_string_lossy(), entry.file_name().to_string_lossy()));
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

// If path already exists, change it a bit until it doesn't. Return the available path
pub fn find_available_path<P: AsRef<Path>>(path: P) -> Result<PathBuf, String> {
  let parent = path.as_ref().parent()
    .ok_or(format!("Error getting parent of: \"{}\"", path.as_ref().to_string_lossy()))?;

  let mut i = 0;
  loop {
    // i is printed in hexadecimal because it looks better
    let current_filename = format!("{}{:x}",
      path.as_ref().file_name()
        .ok_or(format!("Error getting file name of: \"{}\"", path.as_ref().to_string_lossy()))?
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

fn move_folder_child<P: AsRef<Path>>(folder: P) -> Result<(), String> {
  let child_entries = std::fs::read_dir(&folder)
    .map_err(|e| e.to_string())?;

  // If a file or a folder already exists in the destination folder, rename it and save the new name and
  // the original name to this Vector. At the end, after removing the parent folder, rename all elements of this Vector
  let mut collisions: Vec<(PathBuf, PathBuf)> = Vec::new();

  // move its children up one level
  for child in child_entries {
    let child = child
      .map_err(|e| e.to_string())?;
    let from = child.path();
    let to = folder.as_ref().parent()
      .ok_or(format!("Error getting parent of: \"{}\"", folder.as_ref().to_string_lossy()))?
      .join(child.file_name());

    if !to.try_exists().map_err(|e| e.to_string())? {
      std::fs::rename(&from, &to)
        .map_err(|e| e.to_string())?;
    } else {
      // if the children filename already exists on the parent, rename it to a
      // temporal name and, at the end, rename all the temporal names in order to the final names
      let temporal_name: PathBuf = find_available_path(&to)?;
      std::fs::rename(&from, &temporal_name)
        .map_err(|e| e.to_string())?;

      // save the change to the collisions vector
      collisions.push((temporal_name, to));
    }
  }

  // remove the now-empty wrapper dir
  std::fs::remove_dir(&folder)
    .map_err(|e| e.to_string())?;

  // now move all of the filenames that have collided to their original name
  for (src, dst) in collisions.iter() {
    std::fs::rename(&src, &dst)
      .map_err(|e| e.to_string())?;
  }

  Ok(())
}

/// This fuction removes all the common root folders that only contain another folder
/// and unwraps its children to its parent
/// 
/// If applied to the folder `foo` in `/foo/bar/something.txt`, the remainig structure is `/foo/something.txt`
pub fn remove_root_folder<P: AsRef<Path>>(folder: P) -> Result<(), String> {
  loop {
    // list entries
    let mut entries: std::fs::ReadDir = std::fs::read_dir(&folder)
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

pub fn get_file_stem<P: AsRef<Path>>(path: P) -> Result<String, String> {
  path.as_ref()
    .file_stem()
    .ok_or_else(|| format!("Error removing stem from path: \"{}\"", path.as_ref().to_string_lossy()))
    .map(|stem| stem.to_string_lossy().to_string())
}

pub fn file_without_extension<P: AsRef<Path>>(file: P) -> Result<String, String> {
  let mut stem = get_file_stem(&file)?;

  if stem.to_lowercase().ends_with(".tar") {
    stem = get_file_stem(&Path::new(&stem))?;
  }

  Ok(stem)
}
