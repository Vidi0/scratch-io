use thiserror::Error;
use std::path::PathBuf;

// This makes Result<T> behave as Result<T, crate::error::Error>
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
#[error("{context}{kind}")]
pub struct Error {
  #[source]
  pub kind: Box<ErrorKind>,
  pub context: String,
}

// Helper function to add context to an Error
impl Error {
  pub fn add_context(mut self, s: &str) -> Self {
    self.context.push_str(s);
    self.context.push('\n');

    self
  }
}

// Helper function to add context to a Result
pub trait ResultContext<T> {
  fn add_context(self, s: &str) -> Self;
}

impl<T> ResultContext<T> for Result<T> {
  fn add_context(self, s: &str) -> Self {
    self.map_err(|e| e.add_context(s))
  }
}


// Allow creating an Error from any type that implements Into<ErrorKind>
impl<T> From<T> for Error where 
  T: Into<ErrorKind>
{
  fn from(value: T) -> Self {
    Self {
      kind: Box::new(value.into()),
      context: String::new(),
    }
  }
}

#[derive(Error, Debug)]
pub enum ErrorKind {
  #[error("The itch.io server replied with an error:\n{0:#?}")]
  ServerRepliedWithError(Vec<String>),

  #[error("A network request failed:\n{0}")]
  NetworkRequest(#[from] NetworkRequestError),

  #[error("A filesystem operation failed:\n{0}")]
  Filesystem(#[from] FilesystemError),

  #[error(transparent)]
  Parse(#[from] ParseError),

  #[error("Couldn't find any game file executable in: \"{0}\"")]
  ExecutableNotFound(PathBuf),

  #[error(
"File verification failed! The file hash and the hash provided by the server are different.
  File hash:   {file_hash}
  Server hash: {server_hash}"
)]
  MismatchedHashes {
    file_hash: String,
    server_hash: String,
  },

  #[error(
r#"A reCAPTCHA verification is required to continue!
  Go to "{url}" and solve the reCAPTCHA.
  To obtain the token, paste the following command on the developer console:
    console.log(grecaptcha.getResponse())
  Then run the login command again with the --recaptcha-response option."#
)]
  RECaptchaRequired {
    url: String,
  },

  #[error(
r#"The accout has 2 step verification enabled via TOTP
  Run the login command again with the --totp-code={{VERIFICATION_CODE}} option."#
)]
  TOTPRequired,

  #[error("The provided launch action doesn't exist in the manifest: {0}")]
  MissingLaunchAction(String),

  #[error(
r#"The config version of "{config_file}" is not compatible with this scratch-io version!
Update to a newer scratch-io version to be able to load the given config.
  Config version: {config_version}
  Supported version: {supported_version}"#
)]
  IncompatibleConfigVersion {
    config_file: PathBuf,
    config_version: u64,
    supported_version: u64,
  },
}

#[derive(Error, Debug)]
pub enum ParseError {
  #[error("Couldn't parse the request JSON response body:\n{error}\n\n{body}")]
  JSONRequestBody {
    #[source]
    error: serde_json::Error,
    body: String,
  },

  #[error("Couldn't parse the itch manifest: \"{path}\"\n{error}\n\n{text}")]
  ItchManifest {
    #[source]
    error: toml::de::Error,
    path: PathBuf,
    text: String,
  },

  #[error("Invalid configuration file: \"{path}\"\n{error}\n\n{text}")]
  ConfigFile {
    #[source]
    error: toml::de::Error,
    path: PathBuf,
    text: String,
  },

  #[error("Couldn't get the config version: \"{path}\"\n{error}\n\n{text}")]
  ConfigVersion {
    #[source]
    error: toml::de::Error,
    path: PathBuf,
    text: String,
  },

  #[error("Couldn't serialize config into TOML: \"{path}\"\n{error}")]
  SerializeConfig {
    #[source]
    error: toml::ser::Error,
    path: PathBuf,
  },
}

#[derive(Error, Debug)]
pub enum NetworkRequestError {
  #[error("error while sending request, redirect loop was detected or redirect limit was exhausted:\n{0}")]
  Send(#[source] reqwest::Error),

  #[error("Couldn't get the network request response body:\n{0}")]
  Text(#[source] reqwest::Error),

  #[error("Couldn't get next chunk of the network request:\n{0}")]
  GetChunk(#[source] reqwest::Error),

  #[error("Couldn't get the Content Length of the file to download!\n{0:#?}")]
  GetContentLength(reqwest::Response),

  #[error("The HTTP server to download the file from didn't return HTTP code 200 nor 206, so exiting! It returned: {}\n{response:?}", status_code.as_str())]
  InvalidPartialRequestResponse {
    response: reqwest::Response,
    status_code: reqwest::StatusCode,
  }
}

#[derive(Error, Debug)]
pub enum FilesystemError {
  #[error("Couldn't get data from a readable stream:\n{0}")]
  ReadFileBuf(#[source] tokio::io::Error),

  #[error("Couldn't read file as a string: \"{path}\"\n{error}")]
  ReadFileToString {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't write a chunk of data to a file:\n{0}")]
  WriteChunkToFile(#[source] tokio::io::Error),

  #[error("The following path doesn't have a name: \"{0}\"")]
  PathWithoutFilename(PathBuf),

  #[error("The following path doesn't have a parent: \"{0}\"")]
  PathWithoutParent(PathBuf),

  #[error("Couldn't determine the home directory")]
  MissingHomeDirectory,

  #[error("Couldn't determine the config's directory")]
  MissingProjectDirectory,

  #[error("\"{0}\" is not a folder!")]
  ExpectedToBeFolderButIsNot(PathBuf),

  #[error("The folder should be empty but it isn't: \"{0}\"")]
  FolderShouldBeEmpty(PathBuf),

  #[error("The folder should exist but it doesn't: \"{0}\"")]
  FolderShouldExist(PathBuf),

  #[error("Couldn't read directory elements: \"{path}\"\n{error}")]
  ReadDirectory {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't read directory next element: \"{path}\"\n{error}")]
  ReadDirectoryNextEntry {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't remove a empty folder: \"{path}\"\n{error}")]
  RemoveEmptyDir {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't remove a folder and its contents: \"{path}\"\n{error}")]
  RemoveDirWithContents {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't get the canonical (absolute) form of the path. Maybe it doesn't exist: \"{path}\"\n{error}")]
  GetCanonicalPath {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Refusing to remove folder because it is an important path!: \"{0}\"")]
  RefusingToRemoveFolder(PathBuf),

  #[error("Couldn't read the file/directory metadata of: \"{path}\"\n{error}")]
  ReadMetadata {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't change the file/directory permissions of: \"{path}\"\n{error}")]
  ChangePermissions {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't create the folder: \"{path}\"\n{error}")]
  CreateFolder {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't get the file type (file or folder) of: \"{path}\"\n{error}")]
  GetFileType {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't copy file:\n  Source: \"{src}\"\n  Destination: \"{dst}\"\n{error}")]
  CopyFile {
    #[source]
    error: tokio::io::Error,
    src: PathBuf,
    dst: PathBuf,
  },

  #[error("Couldn't remove file:\"{path}\"\n {error}")]
  RemoveFile {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't move file/directory:\n  Source: \"{src}\"\n  Destination: \"{dst}\"\n{error}")]
  Move {
    #[source]
    error: tokio::io::Error,
    src: PathBuf,
    dst: PathBuf,
  },

  #[error("Couldn't check if the path exists: \"{path}\"\n{error}")]
  CheckIfPathExists {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't open the file: \"{path}\"\n{error}")]
  OpenFile {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't write data to a file: \"{path}\"\n{error}")]
  WriteStringToFile {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't extract compressed {format} archive:\n{error}")]
  Extracting {
    #[source]
    error: Box<dyn std::error::Error>,
    format: &'static str,
  },

  #[error("Couldn't create compressed {format} archive reader:\n{error}")]
  OpenExtractionReader {
    #[source]
    error: Box<dyn std::error::Error>,
    format: &'static str,
  },

  #[error("Couldn't remove old partially downloaded file: \"{path}\"\n{error}")]
  RemoveOldFile {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't sync the file data to the disk: \"{path}\"\n{error}")]
  SyncFile {
    #[source]
    error: tokio::io::Error,
    path: PathBuf,
  },

  #[error("Couldn't spawn the child process:\n{0}")]
  CantSpawnProcess(#[source] tokio::io::Error),

  #[error(
"Couldn't spawn the child process because it is not an executable format for this OS
  Maybe a wrapper is missing or the selected game executable isn't the correct one!
{0}"
)]
  CantSpawnProcessExecFormat(#[source] tokio::io::Error),
  
  #[error("Error while awaiting for child exit!:\n{0}")]
  WaitForChildExit(#[source] tokio::io::Error),
}