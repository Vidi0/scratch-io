pub use crate::itch_api::errors::*;

use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FilesystemError {
  #[error(
    "An IO filesystem error occured!
{kind}
{error}"
  )]
  IOError {
    kind: FilesystemIOErrorKind,
    #[source]
    error: std::io::Error,
  },

  #[error(
    "A filesystem error occured!
{0}"
  )]
  OtherError(#[from] OtherFilesystemErrorKind),
}

// TODO: This is temporary while more custom errors aren't implemented
impl From<FilesystemError> for String {
  fn from(value: FilesystemError) -> Self {
    value.to_string()
  }
}

#[derive(Error, Debug)]
pub enum FilesystemIOErrorKind {
  #[error("Couldn't check if the path exists: \"{0}\"")]
  CouldntCheckIfExists(PathBuf),

  #[error("Couldn't read directory elements: \"{0}\"")]
  CouldntReadDirectory(PathBuf),

  #[error("Couldn't read directory next element: \"{0}\"")]
  CouldntReadDirectoryNextEntry(PathBuf),

  #[error("Couldn't get directory element file type: \"{0}\"")]
  CouldntGetFileType(PathBuf),

  #[error(
    "Couldn't get the canonical (absolute) form of the path. Maybe it doesn't exist: \"{0}\""
  )]
  CouldntGetCanonical(PathBuf),

  #[error("Couldn't create the folder: \"{0}\"")]
  CouldntCreateDirectory(PathBuf),

  #[error(
    r#"Couldn't copy file:
  Source: "{from}"
  Destination: "{to}""#
  )]
  CouldntCopyFile { from: PathBuf, to: PathBuf },

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

  #[error("Couldn't read the metadata of the path: \"{0}\"")]
  CouldntReadPathMetadata(PathBuf),

  #[error("Couldn't read the metadata of an open file!")]
  CouldntReadFileMetadata,

  #[error("Couldn't set the permissions of: \"{0}\"")]
  CouldntSetPermissions(PathBuf),

  #[error("Couldn't open the file: \"{0}\"")]
  CouldntOpenFile(PathBuf),

  #[error("Couldn't set file length to: {0}")]
  SetFileLength(u64),

  #[error("Couldn't sync file data to disk!")]
  SyncFile,

  #[error("Couldn't fill a buffer!")]
  CouldntFillBuffer,

  #[error("Couldn't write a buffer to a writer!")]
  CouldntWriteBuffer,

  #[error("Couldn't spawn the child process!")]
  CouldnSpawnProcess,

  #[error("Error while awaiting for child exit!")]
  CouldntWaitForChild,
}

impl FilesystemIOErrorKind {
  /// Returns a closure that attaches this [`FilesystemIOErrorKind`] to a [`std::io::Error`] and returns a [`FilesystemError`]
  pub fn attach(self) -> impl FnOnce(std::io::Error) -> FilesystemError {
    move |error| FilesystemError::IOError { kind: self, error }
  }
}

#[derive(Error, Debug)]
pub enum OtherFilesystemErrorKind {
  #[error("The path contains invalid unicode: \"{}\"", .0.to_string_lossy())]
  InvalidUnicodeOsStr(std::ffi::OsString),

  #[error("The following path doesn't have a filename: \"{0}\"")]
  PathWithoutFilename(PathBuf),

  #[error("The following path doesn't have an extension: \"{0}\"")]
  PathWithoutExtension(PathBuf),

  #[error("The following path doesn't have a parent: \"{0}\"")]
  PathWithoutParent(PathBuf),

  #[error("The following path should be a folder but it is not: \"{0}\"")]
  ShouldBeAFolder(PathBuf),

  #[error("The following path should be an empty folder or not exist but it does: \"{0}\"")]
  ShouldBeEmpty(PathBuf),

  #[error("Refusing to remove folder because it is an important path!: \"{0}\"")]
  RefusingToRemoveFolder(PathBuf),
}

impl OtherFilesystemErrorKind {
  /// Returns a closure that moves this [`OtherFilesystemErrorKind`] into a [`FilesystemError`]
  pub fn attach(self) -> impl FnOnce() -> FilesystemError {
    move || FilesystemError::OtherError(self)
  }
}
