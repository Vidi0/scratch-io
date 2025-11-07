use thiserror::Error;

const ERROR_INVALID_API_KEY: &str = "invalid key";
const ERROR_INVALID_USER_OR_PASSWORD: &str = "Incorrect username or password";
const ERROR_INVALID_CAPTCHA_CODE: &str = "Please correctly complete reCAPTCHA";
const ERROR_INVALID_TOTP_CODE: &str = "invalid code";
const ERROR_TOTP_TOKEN_TIMED_OUT: &str = "two-factor login attempt timed out";
const ERROR_INVALID_TOTP_TOKEN: &str = "invalid token";
const ERROR_INVALID_USER: &str = "invalid user";
const ERROR_INVALID_COLLECTION: &str = "invalid collection";
const ERROR_INVALID_GAME: &str = "invalid game";
const ERROR_INVALID_UPLOAD: &str = "invalid upload";
const ERROR_INVALID_BUILD: &str = "invalid build";
const ERROR_NO_UPGRADE_PATH: &str = "no upgrade path";

/// Error returned from `itch_request_json`
#[derive(Error, Debug)]
#[error("An API call to \"{url}\" failed:\n{kind}")]
pub struct ItchRequestJSONError<T>
where
  T: std::error::Error + std::fmt::Debug,
{
  pub url: String,
  #[source]
  pub kind: ItchRequestJSONErrorKind<T>,
}

#[derive(Error, Debug)]
pub enum ItchRequestJSONErrorKind<T>
where
  T: std::error::Error + std::fmt::Debug,
{
  #[error(
    "Error while sending request, redirect loop was detected or redirect limit was exhausted:\n{0}"
  )]
  CouldntSend(#[source] reqwest::Error),

  #[error("Couldn't get the network request response body:\n{0}")]
  CouldntGetText(#[source] reqwest::Error),

  #[error("Couldn't parse the request response body into JSON:\n{body}\n\n{error}")]
  InvalidJSON {
    body: String,
    #[source]
    error: serde_json::Error,
  },

  #[error("The itch.io API server returned an error:\n{0}")]
  ServerRepliedWithError(T),
}

#[derive(Error, Debug)]
#[error("The provided API key is invalid!")]
pub struct InvalidApiKey;

#[derive(Error, Debug)]
#[error("The username or the password is incorrect.")]
pub struct IncorrectUsernameOrPassword;

#[derive(Error, Debug)]
#[error("The reCAPTCHA response code is incorrect!")]
pub struct IncorrectCaptchaCode;

#[derive(Error, Debug)]
#[error("The TOTP code is incorrect!")]
pub struct IncorrectTOTPCode;

#[derive(Error, Debug)]
#[error(
  "The TOTP token timed out!
Login again with a username and password to get another TOTP token."
)]
pub struct TOTPTokenTimedOut;

#[derive(Error, Debug)]
#[error("The TOTP token is invalid!")]
pub struct InvalidTOTPToken;

#[derive(Error, Debug)]
#[error("The provided user ID is invalid.")]
pub struct InvalidUserID;

#[derive(Error, Debug)]
#[error("The provided collection ID is invalid.")]
pub struct InvalidCollectionID;

#[derive(Error, Debug)]
#[error("The provided game ID is invalid.")]
pub struct InvalidGameID;

#[derive(Error, Debug)]
#[error("The provided upload ID is invalid.")]
pub struct InvalidUploadID;

#[derive(Error, Debug)]
#[error("The provided build ID is invalid.")]
pub struct InvalidBuildID;

#[derive(Error, Debug)]
#[error("No upgrade path was found.")]
pub struct NoUpgradePath;

/// All possible errors returned from the Itch.io API
#[derive(Error, Debug)]
pub enum ApiResponseErrorKind {
  #[error(transparent)]
  InvalidApiKey(#[from] InvalidApiKey),

  #[error(transparent)]
  IncorrectUsernameOrPassword(#[from] IncorrectUsernameOrPassword),

  #[error(transparent)]
  IncorrectCaptchaCode(#[from] IncorrectCaptchaCode),

  #[error(transparent)]
  IncorrectTOTPCode(#[from] IncorrectTOTPCode),

  #[error(transparent)]
  TOTPTokenTimedOut(#[from] TOTPTokenTimedOut),

  #[error(transparent)]
  InvalidTOTPToken(#[from] InvalidTOTPToken),

  #[error(transparent)]
  InvalidUserID(#[from] InvalidUserID),

  #[error(transparent)]
  InvalidCollectionID(#[from] InvalidCollectionID),

  #[error(transparent)]
  InvalidGameID(#[from] InvalidGameID),

  #[error(transparent)]
  InvalidUploadID(#[from] InvalidUploadID),

  #[error(transparent)]
  InvalidBuildID(#[from] InvalidBuildID),

  #[error(transparent)]
  NoUpgradePath(#[from] NoUpgradePath),

  #[error("An unknown error occurred!")]
  Other,
}

impl From<&[String]> for ApiResponseErrorKind {
  fn from(value: &[String]) -> Self {
    match value {
      [v] if v == ERROR_INVALID_API_KEY => Self::InvalidApiKey(InvalidApiKey),
      [v] if v == ERROR_INVALID_USER_OR_PASSWORD => {
        Self::IncorrectUsernameOrPassword(IncorrectUsernameOrPassword)
      }
      [v] if v == ERROR_INVALID_CAPTCHA_CODE => Self::IncorrectCaptchaCode(IncorrectCaptchaCode),
      [v] if v == ERROR_INVALID_TOTP_CODE => Self::IncorrectTOTPCode(IncorrectTOTPCode),
      [v] if v == ERROR_TOTP_TOKEN_TIMED_OUT => Self::TOTPTokenTimedOut(TOTPTokenTimedOut),
      [v] if v == ERROR_INVALID_TOTP_TOKEN => Self::InvalidTOTPToken(InvalidTOTPToken),
      [v] if v == ERROR_INVALID_USER => Self::InvalidUserID(InvalidUserID),
      [v] if v == ERROR_INVALID_COLLECTION => Self::InvalidCollectionID(InvalidCollectionID),
      [v] if v == ERROR_INVALID_GAME => Self::InvalidGameID(InvalidGameID),
      [v] if v == ERROR_INVALID_UPLOAD => Self::InvalidUploadID(InvalidUploadID),
      [v] if v == ERROR_INVALID_BUILD => Self::InvalidBuildID(InvalidBuildID),
      [v] if v == ERROR_NO_UPGRADE_PATH => Self::NoUpgradePath(NoUpgradePath),
      _ => Self::Other,
    }
  }
}

#[derive(Error, Debug)]
#[error("{kind}\n{errors:#?}")]
pub struct ApiResponseError {
  pub errors: Vec<String>,
  #[source]
  pub kind: ApiResponseErrorKind,
}

impl From<Vec<String>> for ApiResponseError {
  fn from(value: Vec<String>) -> Self {
    Self {
      kind: value.as_slice().into(),
      errors: value,
    }
  }
}

/// Common errors to every API call
#[derive(Error, Debug)]
pub enum ApiResponseCommonErrors {
  #[error(transparent)]
  InvalidApiKey(#[from] InvalidApiKey),

  #[error("An unknown error occurred:\n{0:#?}")]
  Other(Vec<String>),
}

impl From<ApiResponseError> for ApiResponseCommonErrors {
  fn from(value: ApiResponseError) -> Self {
    match value.kind {
      ApiResponseErrorKind::InvalidApiKey(v) => Self::InvalidApiKey(v),
      _ => Self::Other(value.errors),
    }
  }
}

/// Errors returned from the login API call
#[derive(Error, Debug)]
pub enum LoginResponseError {
  #[error(transparent)]
  IncorrectUsernameOrPassword(#[from] IncorrectUsernameOrPassword),

  #[error(transparent)]
  IncorrectCaptchaCode(#[from] IncorrectCaptchaCode),

  #[error(transparent)]
  Other(#[from] ApiResponseCommonErrors),
}

impl From<ApiResponseError> for LoginResponseError {
  fn from(value: ApiResponseError) -> Self {
    match value.kind {
      ApiResponseErrorKind::IncorrectUsernameOrPassword(v) => Self::IncorrectUsernameOrPassword(v),
      ApiResponseErrorKind::IncorrectCaptchaCode(v) => Self::IncorrectCaptchaCode(v),
      _ => Self::Other(value.into()),
    }
  }
}

/// Errors returned from the TOTP verification API call
#[derive(Error, Debug)]
pub enum TOTPResponseError {
  #[error(transparent)]
  IncorrectTOTPCode(#[from] IncorrectTOTPCode),

  #[error(transparent)]
  TOTPTokenTimedOut(#[from] TOTPTokenTimedOut),

  #[error(transparent)]
  InvalidTOTPToken(#[from] InvalidTOTPToken),

  #[error(transparent)]
  Other(#[from] ApiResponseCommonErrors),
}

impl From<ApiResponseError> for TOTPResponseError {
  fn from(value: ApiResponseError) -> Self {
    match value.kind {
      ApiResponseErrorKind::IncorrectTOTPCode(v) => Self::IncorrectTOTPCode(v),
      ApiResponseErrorKind::TOTPTokenTimedOut(v) => Self::TOTPTokenTimedOut(v),
      ApiResponseErrorKind::InvalidTOTPToken(v) => Self::InvalidTOTPToken(v),
      _ => Self::Other(value.into()),
    }
  }
}

/// Errors returned from all the API calls that require a user ID as a parameter
#[derive(Error, Debug)]
pub enum UserResponseError {
  #[error(transparent)]
  InvalidUserID(#[from] InvalidUserID),

  #[error(transparent)]
  Other(#[from] ApiResponseCommonErrors),
}

impl From<ApiResponseError> for UserResponseError {
  fn from(value: ApiResponseError) -> Self {
    match value.kind {
      ApiResponseErrorKind::InvalidUserID(v) => Self::InvalidUserID(v),
      _ => Self::Other(value.into()),
    }
  }
}

/// Errors returned from all the API calls that require a collection ID as a parameter
#[derive(Error, Debug)]
pub enum CollectionResponseError {
  #[error(transparent)]
  InvalidCollectionID(#[from] InvalidCollectionID),

  #[error(transparent)]
  Other(#[from] ApiResponseCommonErrors),
}

impl From<ApiResponseError> for CollectionResponseError {
  fn from(value: ApiResponseError) -> Self {
    match value.kind {
      ApiResponseErrorKind::InvalidCollectionID(v) => Self::InvalidCollectionID(v),
      _ => Self::Other(value.into()),
    }
  }
}

/// Errors returned from all the API calls that require a game ID as a parameter
#[derive(Error, Debug)]
pub enum GameResponseError {
  #[error(transparent)]
  InvalidGameID(#[from] InvalidGameID),

  #[error(transparent)]
  Other(#[from] ApiResponseCommonErrors),
}

impl From<ApiResponseError> for GameResponseError {
  fn from(value: ApiResponseError) -> Self {
    match value.kind {
      ApiResponseErrorKind::InvalidGameID(v) => Self::InvalidGameID(v),
      _ => Self::Other(value.into()),
    }
  }
}

/// Errors returned from all the API calls that require an upload ID as a parameter
#[derive(Error, Debug)]
pub enum UploadResponseError {
  #[error(transparent)]
  InvalidUploadID(#[from] InvalidUploadID),

  #[error(transparent)]
  Other(#[from] ApiResponseCommonErrors),
}

impl From<ApiResponseError> for UploadResponseError {
  fn from(value: ApiResponseError) -> Self {
    match value.kind {
      ApiResponseErrorKind::InvalidUploadID(v) => Self::InvalidUploadID(v),
      _ => Self::Other(value.into()),
    }
  }
}

/// Errors returned from all the API calls that require a build ID as a parameter
#[derive(Error, Debug)]
pub enum BuildResponseError {
  #[error(transparent)]
  InvalidBuildID(#[from] InvalidBuildID),

  #[error(transparent)]
  Other(#[from] ApiResponseCommonErrors),
}

impl From<ApiResponseError> for BuildResponseError {
  fn from(value: ApiResponseError) -> Self {
    match value.kind {
      ApiResponseErrorKind::InvalidBuildID(v) => Self::InvalidBuildID(v),
      ApiResponseErrorKind::InvalidUploadID(_) | ApiResponseErrorKind::InvalidGameID(_) => {
        Self::InvalidBuildID(InvalidBuildID)
      }
      _ => Self::Other(value.into()),
    }
  }
}

/// Errors returned from the upgrade path API call
#[derive(Error, Debug)]
pub enum UpgradePathResponseError {
  #[error(transparent)]
  NoUpgradePath(#[from] NoUpgradePath),

  #[error(transparent)]
  Other(#[from] ApiResponseCommonErrors),
}

impl From<ApiResponseError> for UpgradePathResponseError {
  fn from(value: ApiResponseError) -> Self {
    match value.kind {
      ApiResponseErrorKind::NoUpgradePath(v) => Self::NoUpgradePath(v),
      _ => Self::Other(value.into()),
    }
  }
}
