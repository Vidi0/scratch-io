use crate::common::BLOCK_SIZE;
use crate::protos::{pwr, tlc};

use std::fs;
use std::path::{Path, PathBuf};

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

impl std::fmt::Display for pwr::CompressionSettings {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{:?}-q{}", self.algorithm(), self.quality)
  }
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

#[must_use]
pub enum OpenFileStatus {
  Ok {
    file: fs::File,
    container_size: u64,
    disk_size: u64,
  },
  NotFound,
}

impl tlc::File {
  /// Get the number of blocks that the file occupies
  ///
  /// If the file is empty, still count one block for its empty hash
  #[inline]
  #[must_use]
  pub fn block_count(&self) -> u64 {
    (self.size as u64).div_ceil(BLOCK_SIZE).max(1)
  }

  // This function should not be called directly because
  // the container_size variable returned will be 0 instead of the correct value
  fn open_read_from_path(&self, file_path: &Path) -> Result<OpenFileStatus, String> {
    match file_path.metadata() {
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(OpenFileStatus::NotFound),
      Err(e) => Err(format!("Couldn't get file metadata!\n{e}")),
      Ok(m) => fs::File::open(file_path)
        .map(|file| OpenFileStatus::Ok {
          file,
          // Set as 0 and fix in the parent function
          container_size: 0,
          disk_size: m.len(),
        })
        .map_err(|e| {
          format!(
            "Couldn't open file for reading: \"{}\"\n{e}",
            file_path.to_string_lossy()
          )
        }),
    }
  }

  pub fn open_read(&self, build_folder: PathBuf) -> Result<OpenFileStatus, String> {
    let file_path = self.get_path(build_folder)?;
    self.open_read_from_path(&file_path).map(|mut status| {
      // Fix the container size, open_read_from_path doesn't set it!
      if let OpenFileStatus::Ok { container_size, .. } = &mut status {
        *container_size = self.size as u64;
      }

      status
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
  pub fn dump_stdout(&self) {
    // Print the container size
    println!("{}", self.size);

    // Print every file, directory and symlink
    for file in &self.files {
      println!("{file:?}");
    }
    for dir in &self.dirs {
      println!("{dir:?}");
    }
    for sym in &self.symlinks {
      println!("{sym:?}");
    }
  }

  pub fn print_summary(&self, label: &str) {
    println!(
      "{label}: {} files, {} dirs, {} symlinks, total size: {} bytes",
      self.files.len(),
      self.dirs.len(),
      self.symlinks.len(),
      self.size,
    );
  }

  /// Get the number of blocks every file in this container
  /// combined occupies
  ///
  /// If a file is empty, still count one block for its empty hash
  #[inline]
  #[must_use]
  pub fn file_blocks(&self) -> u64 {
    self.files.iter().fold(0, |acc, f| acc + f.block_count())
  }

  /// Get the number of bytes every file in this container
  /// combined occupies
  #[inline]
  #[must_use]
  pub fn file_bytes(&self) -> u64 {
    self.files.iter().fold(0, |acc, f| acc + f.size as u64)
  }

  pub fn get_file(&self, index: usize) -> Result<&tlc::File, String> {
    self
      .files
      .get(index)
      .ok_or_else(|| format!("Invalid old file index: {index}!"))
  }

  pub fn open_file_read(
    &self,
    index: usize,
    build_folder: PathBuf,
  ) -> Result<OpenFileStatus, String> {
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
