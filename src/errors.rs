pub use crate::itch_api::errors::*;

use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FilesystemError {
  #[error(
    "An IO filesystem error occured!
{0}"
  )]
  IOError(#[from] FilesystemIOError),

  #[error(
    "A filesystem error occured!
{0}"
  )]
  OtherError(#[from] OtherFilesystemError),
}

#[derive(Error, Debug)]
#[error(
  "{kind}
{error}"
)]
pub struct FilesystemIOError {
  pub kind: FilesystemIOErrorKind,
  #[source]
  pub error: std::io::Error,
}

#[derive(Error, Debug)]
#[error("{kind}")]
pub struct OtherFilesystemError {
  #[from]
  pub kind: OtherFilesystemErrorKind,
}

#[derive(Error, Debug)]
pub enum FilesystemIOErrorKind {
  #[error("Couldn't check if the path exists: \"{0}\"")]
  CouldntCheckIfExists(PathBuf),

  #[error("Couldn't read directory elements: \"{0}\"")]
  CouldntReadDirectory(PathBuf),

  #[error("Couldn't read directory next element: \"{0}\"")]
  CouldntReadDirectoryNextEntry(PathBuf),

  #[error("Couldn't get the canonical (absolute) form of the path. Maybe it doesn't exist: \"{0}\"")]
  CouldntGetCanonical(PathBuf),

  #[error("Couldn't create the folder: \"{0}\"")]
  CouldntCreateDirectory(PathBuf),

  #[error(
    r#"Couldn't move file/directory:
  Source: "{from}"
  Destination: "{to}""#
  )]
  CouldntMove { from: PathBuf, to: PathBuf },

  #[error("Couldn't remove file: \"{0}\"")]
  CouldntRemoveFile(PathBuf),

  #[error("Couldn't remove a empty folder: \"{0}\"")]
  CouldntRemoveEmptyDir(PathBuf),

  #[error("Couldn't remove a folder and its contents: \"{0}\"")]
  CouldntRemoveDirWithContents(PathBuf),

  #[error("Couldn't read the metadata of: \"{0}\"")]
  CouldntReadMetadata(PathBuf),

  #[error("Couldn't set the permissions of: \"{0}\"")]
  CouldntSetPermissions(PathBuf),
}

impl FilesystemIOErrorKind {
  /// Returns a closure that attaches this `FilesystemIOErrorKind` to a `std::io::Error` and returns a `FilesystemError`
  pub fn attach(self) -> impl FnOnce(std::io::Error) -> FilesystemError {
    move |error| FilesystemError::IOError(FilesystemIOError { kind: self, error })
  }
}

#[derive(Error, Debug)]
pub enum OtherFilesystemErrorKind {
  #[error("The path contains invalid unicode: \"{}\"", .0.to_string_lossy())]
  InvalidUnicodeOsStr(std::ffi::OsString),

  #[error("The following path doesn't have a filename: \"{0}\"")]
  PathWithoutFilename(PathBuf),

  #[error("The following path doesn't have a parent: \"{0}\"")]
  PathWithoutParent(PathBuf),

  #[error("The following path should be a folder but it is not: \"{0}\"")]
  ShouldBeAFolder(PathBuf),

  #[error("Couldn't determine the home directory")]
  MissingHomeDirectory,

  #[error("Refusing to remove folder because it is an important path!: \"{0}\"")]
  RefusingToRemoveFolder(PathBuf),
}

impl OtherFilesystemErrorKind {
  /// Returns a closure that moves this `OtherFilesystemErrorKind` into a `FilesystemError`
  pub fn attach(self) -> impl FnOnce() -> FilesystemError {
    move || FilesystemError::OtherError(OtherFilesystemError { kind: self })
  }
}
