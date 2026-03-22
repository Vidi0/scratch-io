use super::{ContainerBackedPool, Pool, PoolError, SeekablePool, WritablePool};
use crate::protos::tlc;

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// <https://github.com/itchio/wharf/blob/189a01902d172b3297051fab12d5d4db2c620e1d/pwr/constants.go#L30>
const MIN_MODE: u32 = 0o644;
const MAX_MODE: u32 = 0o777;

/// Clamp the given mode between the minimum and maximum
///
/// Clamping the mode ensures that it is valid
#[inline]
#[must_use]
fn mask_mode(mode: u32) -> u32 {
  (mode & MAX_MODE) | MIN_MODE
}

fn set_permissions(path: &Path, mode: u32) -> Result<(), PoolError> {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;

    let exists = fs::exists(path)?;
    if !exists {
      return Ok(());
    }

    // Apply the mode mask to set at least the mask permissions
    let mode = mask_mode(mode);
    let mut permissions = fs::metadata(path)?.permissions();

    if permissions.mode() != mode {
      permissions.set_mode(mode);
      fs::set_permissions(path, permissions)?;
    }
  }

  Ok(())
}

fn symlink(path: &Path, destination: &str) -> Result<(), PoolError> {
  let exists = fs::exists(path)?;
  if exists {
    fs::remove_file(path)?;
  }

  #[cfg(unix)]
  {
    std::os::unix::fs::symlink(destination, path)?;
  }

  #[cfg(windows)]
  {
    let metadata = fs::metadata(destination)?;

    if metadata.is_dir() {
      std::os::windows::fs::symlink_dir(destination, path)?;
    } else {
      std::os::windows::fs::symlink_file(destination, path)?;
    }
  }

  Ok(())
}

fn path_safe_push(base: &mut PathBuf, extension: &Path) -> Result<(), PoolError> {
  for comp in extension.components() {
    match comp {
      std::path::Component::Normal(p) => base.push(p),
      std::path::Component::CurDir => (),

      // Any other component is not safe!
      _ => {
        return Err(PoolError::Io(io::Error::new(
          io::ErrorKind::InvalidInput,
          format!("The path is not safe, it contains an invalid component: {comp:?}"),
        )));
      }
    }
  }

  Ok(())
}

trait ContainerItem {
  fn mode(&self) -> u32;
  fn path(&self) -> &str;

  fn get_path(&self, mut build_folder: PathBuf) -> Result<PathBuf, PoolError> {
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

/// A pool backed by a folder on disk, mirroring the structure of a [`tlc::Container`]
///
/// Each entry is located by resolving its path from the container metadata
/// against the base folder. The folder structure is created on construction
/// to match the container's declared directories, files and symlinks.
pub struct ContainerPool<'container, 'path> {
  container: &'container tlc::Container,
  base_path: &'path Path,
}

impl<'container, 'path> ContainerPool<'container, 'path> {
  fn create_directories(&self) -> Result<(), PoolError> {
    // Create build root directory
    fs::create_dir_all(self.base_path)?;

    // Iterate over the folders in the container and create them
    for dir in &self.container.dirs {
      let dir_path = dir.get_path(self.base_path.to_owned())?;

      // This function call will do nothing if the directory already exists
      fs::create_dir_all(&dir_path)?;
    }

    Ok(())
  }

  fn create_files(&self) -> Result<(), PoolError> {
    // Iterate over the files in the container and create them
    for file in &self.container.files {
      let file_path = file.get_path(self.base_path.to_owned())?;

      // The file handle will be dropped just after creating the file
      // If the file already exists, it won't be touched
      fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)?;
    }

    Ok(())
  }

  fn create_symlinks(&self) -> Result<(), PoolError> {
    // Iterate over the symlinks in the container and create them
    for sym in &self.container.symlinks {
      let sym_path = sym.get_path(self.base_path.to_owned())?;

      // Create the symlink
      symlink(&sym_path, &sym.dest)?;
    }

    Ok(())
  }

  fn apply_permissions(&self) -> Result<(), PoolError> {
    for file in &self.container.files {
      set_permissions(&file.get_path(self.base_path.to_owned())?, file.mode())?;
    }

    for dir in &self.container.dirs {
      set_permissions(&dir.get_path(self.base_path.to_owned())?, dir.mode())?;
    }

    for sym in &self.container.symlinks {
      set_permissions(&sym.get_path(self.base_path.to_owned())?, sym.mode())?;
    }

    Ok(())
  }

  fn get_file(&self, entry_index: usize) -> Result<&tlc::File, PoolError> {
    self
      .container
      .files
      .get(entry_index)
      .ok_or(PoolError::InvalidEntryIndex(entry_index))
  }

  fn get_path(&self, entry_index: usize) -> Result<PathBuf, PoolError> {
    self
      .get_file(entry_index)
      .and_then(|f| f.get_path(self.base_path.to_owned()))
  }
}

impl<'container, 'path> ContainerPool<'container, 'path> {
  /// Open an existing folder as a [`ContainerPool`] without creating anything
  pub fn open(container: &'container tlc::Container, base_path: &'path Path) -> Self {
    Self {
      container,
      base_path,
    }
  }

  /// Create the folder structure on disk and return a new [`ContainerPool`]
  ///
  /// Creates all directories, files and symlinks described in the container
  /// under `base_path`, applying the correct permissions to each.
  pub fn create(
    container: &'container tlc::Container,
    base_path: &'path Path,
  ) -> Result<Self, PoolError> {
    let pool = Self::open(container, base_path);

    pool.create_directories()?;
    pool.create_files()?;
    pool.create_symlinks()?;
    pool.apply_permissions()?;

    Ok(pool)
  }
}

impl Pool for ContainerPool<'_, '_> {
  type Reader<'a>
    = File
  where
    Self: 'a;

  fn entry_count(&self) -> usize {
    self.container.files.len()
  }

  fn get_size(&self, entry_index: usize) -> Result<Option<u64>, PoolError> {
    match fs::metadata(self.get_path(entry_index)?) {
      Ok(m) => Ok(Some(m.len())),
      Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
      Err(e) => Err(e.into()),
    }
  }

  fn get_reader(&mut self, entry_index: usize) -> Result<Self::Reader<'_>, PoolError> {
    let path = self.get_path(entry_index)?;
    Ok(File::open(&path)?)
  }
}

impl ContainerBackedPool for ContainerPool<'_, '_> {
  fn get_container_size(&self, entry_index: usize) -> Result<u64, PoolError> {
    self.get_file(entry_index).map(|f| f.size as u64)
  }
}

impl SeekablePool for ContainerPool<'_, '_> {
  type SeekableReader<'a>
    = Self::Reader<'a>
  where
    Self: 'a;

  fn get_seek_reader(&mut self, entry_index: usize) -> Result<Self::SeekableReader<'_>, PoolError> {
    self.get_reader(entry_index)
  }
}

impl WritablePool for ContainerPool<'_, '_> {
  type Writer<'a>
    = File
  where
    Self: 'a;

  fn truncate(&mut self, entry_index: usize, size: u64) -> Result<(), PoolError> {
    let Some(current_size) = self.get_size(entry_index)? else {
      return Err(PoolError::Io(io::Error::new(
        io::ErrorKind::NotFound,
        "Couldn't truncate file in ContainerPool if the file is missing!",
      )));
    };

    if current_size < size {
      return Err(PoolError::Io(io::Error::new(
        io::ErrorKind::InvalidInput,
        "Can't truncate file to a size greater than the current one!",
      )));
    }

    Ok(self.get_writer(entry_index)?.set_len(size)?)
  }

  fn get_writer(&mut self, entry_index: usize) -> Result<Self::Writer<'_>, PoolError> {
    let path = self.get_path(entry_index)?;
    Ok(OpenOptions::new().create(true).append(true).open(&path)?)
  }
}
