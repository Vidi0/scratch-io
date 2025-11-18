use crate::errors::{
  FilesystemError, FilesystemIOErrorKind as IOErr, NetworkErrorKind as NetErr,
  OtherFilesystemErrorKind as OtherErr,
};

use std::path::{Path, PathBuf};
use tokio::fs;

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

/// [`tokio::fs::try_exists`]
pub async fn exists(path: &Path) -> Result<bool, FilesystemError> {
  fs::try_exists(path)
    .await
    .map_err(IOErr::CouldntCheckIfExists(path.to_owned()).attach())
}

/// [`std::path::Path::parent`]
pub fn parent(path: &Path) -> Result<&Path, FilesystemError> {
  path
    .parent()
    .ok_or_else(OtherErr::PathWithoutParent(path.to_owned()).attach())
}

/// [`tokio::fs::read_dir`]
pub async fn read_dir(path: &Path) -> Result<fs::ReadDir, FilesystemError> {
  fs::read_dir(path)
    .await
    .map_err(IOErr::CouldntReadDirectory(path.to_owned()).attach())
}

/// [`tokio::fs::ReadDir::next_entry`]
pub async fn next_entry(
  read_dir: &mut fs::ReadDir,
  path: &Path,
) -> Result<Option<fs::DirEntry>, FilesystemError> {
  read_dir
    .next_entry()
    .await
    .map_err(IOErr::CouldntReadDirectoryNextEntry(path.to_owned()).attach())
}

/// [`tokio::fs::DirEntry::file_type`]
pub async fn file_type(
  dir_entry: &fs::DirEntry,
  path: &Path,
) -> Result<std::fs::FileType, FilesystemError> {
  dir_entry
    .file_type()
    .await
    .map_err(IOErr::CouldntGetFileType(path.to_owned()).attach())
}

/// [`tokio::fs::canonicalize`]
pub async fn get_canonical_path(path: &Path) -> Result<PathBuf, FilesystemError> {
  fs::canonicalize(path)
    .await
    .map_err(IOErr::CouldntGetCanonical(path.to_owned()).attach())
}

/// [`directories::BaseDirs::new`]
pub fn get_basedirs() -> Result<directories::BaseDirs, FilesystemError> {
  directories::BaseDirs::new().ok_or_else(OtherErr::MissingHomeDirectory.attach())
}

/// [`tokio::fs::create_dir_all`]
pub async fn create_dir(path: &Path) -> Result<(), FilesystemError> {
  fs::create_dir_all(path)
    .await
    .map_err(IOErr::CouldntCreateDirectory(path.to_owned()).attach())
}

/// [`tokio::fs::copy`]
pub async fn copy_file(from: &Path, to: &Path) -> Result<u64, FilesystemError> {
  fs::copy(from, to).await.map_err(
    IOErr::CouldntCopyFile {
      from: from.to_owned(),
      to: to.to_owned(),
    }
    .attach(),
  )
}

/// [`tokio::fs::rename`]
pub async fn rename(from: &Path, to: &Path) -> Result<(), FilesystemError> {
  fs::rename(from, to).await.map_err(
    IOErr::CouldntMove {
      from: from.to_owned(),
      to: to.to_owned(),
    }
    .attach(),
  )
}

/// [`tokio::fs::remove_file`]
pub async fn remove_file(path: &Path) -> Result<(), FilesystemError> {
  fs::remove_file(path)
    .await
    .map_err(IOErr::CouldntRemoveFile(path.to_owned()).attach())
}

/// [`tokio::fs::remove_dir`]
pub async fn remove_empty_dir(path: &Path) -> Result<(), FilesystemError> {
  fs::remove_dir(path)
    .await
    .map_err(IOErr::CouldntRemoveEmptyDir(path.to_owned()).attach())
}

/// [`tokio::fs::remove_dir_all`]
pub async fn remove_dir_all(path: &Path) -> Result<(), FilesystemError> {
  fs::remove_dir_all(path)
    .await
    .map_err(IOErr::CouldntRemoveDirWithContents(path.to_owned()).attach())
}

/// [`tokio::fs::metadata`]
pub async fn read_path_metadata(path: &Path) -> Result<std::fs::Metadata, FilesystemError> {
  fs::metadata(path)
    .await
    .map_err(IOErr::CouldntReadPathMetadata(path.to_owned()).attach())
}

/// [`tokio::fs::File::metadata`]
pub async fn read_file_metadata(file: &fs::File) -> Result<std::fs::Metadata, FilesystemError> {
  file
    .metadata()
    .await
    .map_err(IOErr::CouldntReadFileMetadata.attach())
}

/// Checks if a given path represents a directory on the filesystem
///
/// # Errors
///
/// If the filesystem operation fails
pub async fn is_dir(path: &Path) -> Result<bool, FilesystemError> {
  read_path_metadata(path)
    .await
    .map(|metadata| metadata.is_dir())
}

/// Checks if a folder is empty
///
/// # Errors
///
/// If any filesystem operation fails
pub async fn is_folder_empty(folder: &Path) -> Result<bool, FilesystemError> {
  if is_dir(folder).await? {
    if next_entry(&mut read_dir(folder).await?, folder)
      .await?
      .is_none()
    {
      Ok(true)
    } else {
      Ok(false)
    }
  } else if exists(folder).await? {
    Err(OtherErr::ShouldBeAFolder(folder.to_owned()).into())
  } else {
    Ok(true)
  }
}

/// Ensume `path` is a folder
///
/// # Errors
///
/// If `path` is not a directory
pub async fn ensure_is_dir(path: &Path) -> Result<(), FilesystemError> {
  if is_dir(path).await? {
    Ok(())
  } else {
    Err(OtherErr::ShouldBeAFolder(path.to_owned()).into())
  }
}

/// Ensume `path` doesn't exist or is an empty folder
///
/// # Errors
///
/// If `path` is a file
pub async fn ensure_is_empty(path: &Path) -> Result<(), FilesystemError> {
  if is_folder_empty(path).await? {
    Ok(())
  } else {
    Err(OtherErr::ShouldBeEmpty(path.to_owned()).into())
  }
}

/// [`tokio::fs::set_permissions`]
pub async fn set_permissions(
  path: &Path,
  permissions: std::fs::Permissions,
) -> Result<(), FilesystemError> {
  fs::set_permissions(path, permissions)
    .await
    .map_err(IOErr::CouldntSetPermissions(path.to_owned()).attach())
}

/// [`tokio::fs::OpenOptions::open`]
pub async fn open_file(
  path: &Path,
  options: &mut fs::OpenOptions,
) -> Result<fs::File, FilesystemError> {
  options
    .open(path)
    .await
    .map_err(IOErr::CouldntOpenFile(path.to_owned()).attach())
}

/// [`tokio::fs::File::set_len`]
pub async fn set_file_len(file: &fs::File, size: u64) -> Result<(), FilesystemError> {
  file
    .set_len(size)
    .await
    .map_err(IOErr::SetFileLength(size).attach())
}

/// Make the provided path executable (on Unix targets)
///
/// # Errors
///
/// If the filesystem operation fails
#[cfg_attr(not(unix), allow(unused_variables))]
pub async fn make_executable(path: &Path) -> Result<(), FilesystemError> {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;

    let metadata = read_path_metadata(path).await?;
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

/// [`tokio::io::AsyncBufReadExt::fill_buf`]
pub async fn fill_buffer(
  buf: &mut (impl tokio::io::AsyncBufReadExt + Unpin),
) -> Result<&[u8], FilesystemError> {
  buf
    .fill_buf()
    .await
    .map_err(IOErr::CouldntFillAsyncBuffer.attach())
}

/// [`tokio::io::AsyncWriteExt::write_all`]
pub async fn write_all(
  writer: &mut (impl tokio::io::AsyncWriteExt + Unpin),
  buffer: &[u8],
) -> Result<(), FilesystemError> {
  writer
    .write_all(buffer)
    .await
    .map_err(IOErr::CouldntWriteBuffer.attach())
}

/// [`futures_util::stream::StreamExt::next`]
pub async fn next_chunk<T>(
  stream: &mut (impl futures_util::Stream<Item = Result<T, reqwest::Error>> + Unpin),
) -> Result<Option<T>, FilesystemError> {
  use futures_util::StreamExt;

  match stream.next().await {
    None => Ok(None),
    Some(result) => result.map(Some).map_err(NetErr::CouldntReadChunk.attach()),
  }
}
