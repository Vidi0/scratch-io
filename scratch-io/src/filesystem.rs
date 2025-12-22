use crate::errors::{
  FilesystemError, FilesystemIOErrorKind as IOErr, OtherFilesystemErrorKind as OtherErr,
};

use std::fs;
use std::path::{Path, PathBuf};

/// [`std::ffi::OsStr::to_str`]
pub fn os_str_as_str(os_str: &std::ffi::OsStr) -> Result<&str, FilesystemError> {
  os_str
    .to_str()
    .ok_or_else(OtherErr::InvalidUnicodeOsStr(os_str.to_owned()).attach())
}

/// [`std::path::Path::file_name`]
pub fn get_file_name(path: &Path) -> Result<&str, FilesystemError> {
  path
    .file_name()
    .ok_or_else(OtherErr::PathWithoutFilename(path.to_owned()).attach())
    .and_then(|stem| os_str_as_str(stem))
}

/// [`std::path::Path::file_stem`]
pub fn get_file_stem(path: &Path) -> Result<&str, FilesystemError> {
  path
    .file_stem()
    .ok_or_else(OtherErr::PathWithoutFilename(path.to_owned()).attach())
    .and_then(|stem| os_str_as_str(stem))
}

/// [`std::path::Path::extension`]
pub fn get_file_extension(path: &Path) -> Result<&str, FilesystemError> {
  path
    .extension()
    .ok_or_else(OtherErr::PathWithoutExtension(path.to_owned()).attach())
    .and_then(|extension| os_str_as_str(extension))
}

/// [`std::fs::try_exists`]
pub fn exists(path: &Path) -> Result<bool, FilesystemError> {
  fs::exists(path).map_err(IOErr::CouldntCheckIfExists(path.to_owned()).attach())
}

/// [`std::path::Path::parent`]
pub fn parent(path: &Path) -> Result<&Path, FilesystemError> {
  path
    .parent()
    .ok_or_else(OtherErr::PathWithoutParent(path.to_owned()).attach())
}

/// [`std::fs::read_dir`]
pub fn read_dir(path: &Path) -> Result<fs::ReadDir, FilesystemError> {
  fs::read_dir(path).map_err(IOErr::CouldntReadDirectory(path.to_owned()).attach())
}

/// [`std::fs::ReadDir::next_entry`]
pub fn next_entry(
  read_dir: &mut fs::ReadDir,
  path: &Path,
) -> Result<Option<fs::DirEntry>, FilesystemError> {
  match read_dir.next() {
    None => Ok(None),
    Some(entry) => entry
      .map(Some)
      .map_err(IOErr::CouldntReadDirectoryNextEntry(path.to_owned()).attach()),
  }
}

/// [`std::fs::DirEntry::file_type`]
pub fn file_type(
  dir_entry: &fs::DirEntry,
  path: &Path,
) -> Result<std::fs::FileType, FilesystemError> {
  dir_entry
    .file_type()
    .map_err(IOErr::CouldntGetFileType(path.to_owned()).attach())
}

/// [`std::fs::canonicalize`]
pub fn get_canonical_path(path: &Path) -> Result<PathBuf, FilesystemError> {
  fs::canonicalize(path).map_err(IOErr::CouldntGetCanonical(path.to_owned()).attach())
}

/// [`std::fs::create_dir_all`]
pub fn create_dir(path: &Path) -> Result<(), FilesystemError> {
  fs::create_dir_all(path).map_err(IOErr::CouldntCreateDirectory(path.to_owned()).attach())
}

/// [`std::fs::copy`]
pub fn copy_file(from: &Path, to: &Path) -> Result<u64, FilesystemError> {
  fs::copy(from, to).map_err(
    IOErr::CouldntCopyFile {
      from: from.to_owned(),
      to: to.to_owned(),
    }
    .attach(),
  )
}

/// [`std::fs::rename`]
pub fn rename(from: &Path, to: &Path) -> Result<(), FilesystemError> {
  fs::rename(from, to).map_err(
    IOErr::CouldntMove {
      from: from.to_owned(),
      to: to.to_owned(),
    }
    .attach(),
  )
}

/// [`std::fs::remove_file`]
pub fn remove_file(path: &Path) -> Result<(), FilesystemError> {
  fs::remove_file(path).map_err(IOErr::CouldntRemoveFile(path.to_owned()).attach())
}

/// [`std::fs::remove_dir`]
pub fn remove_empty_dir(path: &Path) -> Result<(), FilesystemError> {
  fs::remove_dir(path).map_err(IOErr::CouldntRemoveEmptyDir(path.to_owned()).attach())
}

/// [`std::fs::remove_dir_all`]
pub fn remove_dir_all(path: &Path) -> Result<(), FilesystemError> {
  fs::remove_dir_all(path).map_err(IOErr::CouldntRemoveDirWithContents(path.to_owned()).attach())
}

/// [`std::fs::metadata`]
pub fn read_path_metadata(path: &Path) -> Result<std::fs::Metadata, FilesystemError> {
  fs::metadata(path).map_err(IOErr::CouldntReadPathMetadata(path.to_owned()).attach())
}

/// [`std::fs::File::metadata`]
pub fn read_file_metadata(file: &fs::File) -> Result<std::fs::Metadata, FilesystemError> {
  file
    .metadata()
    .map_err(IOErr::CouldntReadFileMetadata.attach())
}

/// Checks if a given path represents a directory on the filesystem
///
/// Returns none if the path doesn't exist
///
/// # Errors
///
/// If the filesystem operation fails
pub fn is_dir(path: &Path) -> Result<Option<bool>, FilesystemError> {
  if exists(path)? {
    read_path_metadata(path).map(|metadata| Some(metadata.is_dir()))
  } else {
    Ok(None)
  }
}

/// Checks if a folder is empty
///
/// # Errors
///
/// If any filesystem operation fails
pub fn is_folder_empty(folder: &Path) -> Result<bool, FilesystemError> {
  match is_dir(folder)? {
    // If it doesn't exist, return true (is empty)
    None => Ok(true),
    // If it isn't a folder, return an error
    Some(false) => Err(OtherErr::ShouldBeAFolder(folder.to_owned()).into()),
    // If it is a folder, check if it's empty
    Some(true) => match next_entry(&mut read_dir(folder)?, folder)? {
      // It it's empty, return true
      None => Ok(true),
      // If it's not empty, return false
      Some(_) => Ok(false),
    },
  }
}

/// Ensure `path` is a folder
///
/// # Errors
///
/// If `path` is not a directory
pub fn ensure_is_dir(path: &Path) -> Result<(), FilesystemError> {
  if is_dir(path)? == Some(true) {
    Ok(())
  } else {
    Err(OtherErr::ShouldBeAFolder(path.to_owned()).into())
  }
}

/// Ensure `path` doesn't exist or is an empty folder
///
/// # Errors
///
/// If `path` is a file
pub fn ensure_is_empty(path: &Path) -> Result<(), FilesystemError> {
  if is_folder_empty(path)? {
    Ok(())
  } else {
    Err(OtherErr::ShouldBeEmpty(path.to_owned()).into())
  }
}

/// [`std::fs::set_permissions`]
pub fn set_permissions(
  path: &Path,
  permissions: std::fs::Permissions,
) -> Result<(), FilesystemError> {
  fs::set_permissions(path, permissions)
    .map_err(IOErr::CouldntSetPermissions(path.to_owned()).attach())
}

/// [`std::fs::OpenOptions::open`]
pub fn open_file(path: &Path, options: &fs::OpenOptions) -> Result<fs::File, FilesystemError> {
  options
    .open(path)
    .map_err(IOErr::CouldntOpenFile(path.to_owned()).attach())
}

/// [`std::fs::File::set_len`]
pub fn set_file_len(file: &fs::File, size: u64) -> Result<(), FilesystemError> {
  file
    .set_len(size)
    .map_err(IOErr::SetFileLength(size).attach())
}

/// [`std::fs::File::sync_all`]
pub fn file_sync_all(file: &fs::File) -> Result<(), FilesystemError> {
  file.sync_all().map_err(IOErr::SyncFile.attach())
}

/// Make the provided path executable (on Unix targets)
///
/// # Errors
///
/// If the filesystem operation fails
#[cfg_attr(not(unix), allow(unused_variables))]
pub fn make_executable(path: &Path) -> Result<(), FilesystemError> {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;

    let metadata = read_path_metadata(path)?;
    let mut permissions = metadata.permissions();
    let mode = permissions.mode();

    // If all the executable bits are already set, return Ok()
    if mode & 0o111 == 0o111 {
      return Ok(());
    }

    // Otherwise, add execute bits
    permissions.set_mode(mode | 0o111);

    set_permissions(path, permissions)?;
  }

  Ok(())
}

/// [`std::io::BufRead::fill_buf`]
pub fn fill_buffer(buf: &mut impl std::io::BufRead) -> Result<&[u8], FilesystemError> {
  buf.fill_buf().map_err(IOErr::CouldntFillBuffer.attach())
}

/// [`std::io::Write::write_all`]
pub fn write_all(writer: &mut impl std::io::Write, buffer: &[u8]) -> Result<(), FilesystemError> {
  writer
    .write_all(buffer)
    .map_err(IOErr::CouldntWriteBuffer.attach())
}

/// [`std::process::Command::spawn`]
pub fn spawn_command(
  command: &mut std::process::Command,
) -> Result<std::process::Child, FilesystemError> {
  command.spawn().map_err(IOErr::CouldnSpawnProcess.attach())
}

/// [`std::process::Child::wait`]
pub fn wait_child(
  child: &mut std::process::Child,
) -> Result<std::process::ExitStatus, FilesystemError> {
  child.wait().map_err(IOErr::CouldntWaitForChild.attach())
}
