use crate::protos::tlc;

use std::fs;
use std::path::{Path, PathBuf};

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L33>
pub const BLOCK_SIZE: u64 = 64 * 1024;

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L30>
const MIN_MODE: u32 = 0o644;
const MAX_MODE: u32 = 0o777;

/// Clamp the given mode between the minimum and maximum
///
/// Clamping the mode ensures that it is valid
#[inline]
#[must_use]
pub fn mask_mode(mode: u32) -> u32 {
  (mode & MAX_MODE) | MIN_MODE
}

/// Get the number of blocks that a file of a given size occupies
///
/// If the file is empty, still count one block for its empty hash
#[inline]
#[must_use]
pub fn file_blocks(size: u64) -> u64 {
  size.div_ceil(BLOCK_SIZE).max(1)
}

fn set_permissions(path: &Path, mode: u32) -> Result<(), String> {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;

    let exists = fs::exists(path).map_err(|e| {
      format!(
        "Couldn't check if the path exists: \"{}\"\n{e}",
        path.to_string_lossy()
      )
    })?;

    if !exists {
      return Ok(());
    }

    // Apply the mode mask to set at least the mask permissions
    let mode = mask_mode(mode);

    let mut permissions = fs::metadata(path)
      .map_err(|e| {
        format!(
          "Couldn't read path metadata: \"{}\"\n{e}",
          path.to_string_lossy()
        )
      })?
      .permissions();

    if permissions.mode() != mode {
      permissions.set_mode(mode);

      fs::set_permissions(path, permissions).map_err(|e| {
        format!(
          "Couldn't change path permissions: \"{}\"\n{e}",
          path.to_string_lossy()
        )
      })?;
    }
  }

  Ok(())
}

fn symlink(path: &Path, destination: &str) -> Result<(), String> {
  let exists = fs::exists(path).map_err(|e| {
    format!(
      "Couldn't check if the symlink exists: \"{}\"\n{e}",
      path.to_string_lossy()
    )
  })?;

  if exists {
    fs::remove_file(path).map_err(|e| {
      format!(
        "Couldn't remove old symlink: \"{}\"\n{e}",
        path.to_string_lossy()
      )
    })?;
  }

  #[cfg(unix)]
  {
    std::os::unix::fs::symlink(destination, path).map_err(|e| {
      format!(
        "Couldn't create symlink
  Link: {}
  Original: {}
{e}",
        path.to_string_lossy(),
        destination,
      )
    })?;
  }

  #[cfg(windows)]
  {
    let metadata = fs::metadata(destination)
      .map_err(|e| format!("Couldn't get symlink destination metadata"))?;

    if metadata.is_dir() {
      std::os::windows::fs::symlink_dir(destination, path).map_err(|e| {
        format!(
          "Couldn't create directory symlink
  Link: {}
  Original: {}
{e}",
          path.to_string_lossy(),
          destination,
        )
      })?;
    } else {
      std::os::windows::fs::symlink_file(destination, path).map_err(|e| {
        format!(
          "Couldn't create file symlink
  Link: {}
  Original: {}
{e}",
          path.to_string_lossy(),
          destination,
        )
      })?;
    }
  }

  Ok(())
}

fn path_safe_push(base: &mut PathBuf, extension: &Path) -> Result<(), String> {
  for comp in extension.components() {
    match comp {
      std::path::Component::Normal(p) => base.push(p),
      std::path::Component::CurDir => (),

      // Any other component is not safe!
      _ => return Err(format!("The extension is not safe! It contains: {comp:?}")),
    }
  }

  Ok(())
}

pub trait ContainerItem {
  fn mode(&self) -> u32;
  fn path(&self) -> &str;

  fn get_path(&self, mut build_folder: PathBuf) -> Result<PathBuf, String> {
    path_safe_push(&mut build_folder, Path::new(self.path()))?;
    Ok(build_folder)
  }
}

impl ContainerItem for tlc::Dir {
  fn mode(&self) -> u32 {
    self.mode
  }

  fn path(&self) -> &str {
    &self.path
  }
}

impl ContainerItem for tlc::File {
  fn mode(&self) -> u32 {
    self.mode
  }

  fn path(&self) -> &str {
    &self.path
  }
}

impl ContainerItem for tlc::Symlink {
  fn mode(&self) -> u32 {
    self.mode
  }

  fn path(&self) -> &str {
    &self.path
  }
}

impl tlc::File {
  pub fn open_read(&self, build_folder: PathBuf) -> Result<fs::File, String> {
    let file_path = self.get_path(build_folder)?;

    fs::File::open(&file_path).map_err(|e| {
      format!(
        "Couldn't open file for reading: \"{}\"\n{e}",
        file_path.to_string_lossy()
      )
    })
  }

  pub fn open_write(&self, build_folder: PathBuf) -> Result<fs::File, String> {
    let file_path = self.get_path(build_folder)?;

    fs::OpenOptions::new()
      .write(true)
      .truncate(true)
      .open(&file_path)
      .map_err(|e| {
        format!(
          "Couldn't open file for writting: \"{}\"\n{e}",
          file_path.to_string_lossy()
        )
      })
  }
}

impl tlc::Container {
  pub fn get_file(&self, index: usize) -> Result<&tlc::File, String> {
    self
      .files
      .get(index)
      .ok_or_else(|| format!("Invalid old file index: {index}!"))
  }

  pub fn open_file_read(&self, index: usize, build_folder: PathBuf) -> Result<fs::File, String> {
    let file = self.get_file(index)?;
    file.open_read(build_folder)
  }

  pub fn open_file_write(&self, index: usize, build_folder: PathBuf) -> Result<fs::File, String> {
    let file = self.get_file(index)?;
    file.open_write(build_folder)
  }

  pub fn create_directories(&self, build_folder: &Path) -> Result<(), String> {
    // Create build root directory
    fs::create_dir_all(build_folder).map_err(|e| {
      format!(
        "Couldn't create build directory: \"{}\"\n{e}",
        build_folder.to_string_lossy()
      )
    })?;

    // Iterate over the folders in the container and create them
    for dir in &self.dirs {
      let dir_path = dir.get_path(build_folder.to_owned())?;

      // This function call will do nothing if the directory already exists
      fs::create_dir_all(&dir_path).map_err(|e| {
        format!(
          "Couldn't create directory: \"{}\"\n{e}",
          dir_path.to_string_lossy()
        )
      })?;
    }

    Ok(())
  }

  pub fn create_files(&self, build_folder: &Path) -> Result<(), String> {
    // Iterate over the files in the container and create them
    for file in &self.files {
      let file_path = file.get_path(build_folder.to_owned())?;

      // The file handle will be dropped just after creating the file
      // If the file already exists, it won't be touched
      fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .map_err(|e| {
          format!(
            "Couldn't open file for writting: \"{}\"\n{e}",
            file_path.to_string_lossy()
          )
        })?;
    }

    Ok(())
  }

  pub fn create_symlinks(&self, build_folder: &Path) -> Result<(), String> {
    // Iterate over the symlinks in the container and create them
    for sym in &self.symlinks {
      let sym_path = sym.get_path(build_folder.to_owned())?;

      // Create the symlink
      symlink(&sym_path, &sym.dest)?;
    }

    Ok(())
  }

  pub fn apply_permissions(&self, build_folder: &Path) -> Result<(), String> {
    for file in &self.files {
      set_permissions(&file.get_path(build_folder.to_owned())?, file.mode())?;
    }

    for dir in &self.dirs {
      set_permissions(&dir.get_path(build_folder.to_owned())?, dir.mode())?;
    }

    for sym in &self.symlinks {
      set_permissions(&sym.get_path(build_folder.to_owned())?, sym.mode())?;
    }

    Ok(())
  }

  pub fn create(&self, build_folder: &Path) -> Result<(), String> {
    self.create_directories(build_folder)?;
    self.create_files(build_folder)?;
    self.create_symlinks(build_folder)?;

    self.apply_permissions(build_folder)
  }
}
