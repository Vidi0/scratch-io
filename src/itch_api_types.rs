use serde::{Serialize, Deserialize};
use time::{OffsetDateTime, serde::rfc3339};
use thiserror::Error;

const ITCH_API_V1_BASE_URL: &str = "https://itch.io/api/1";
const ITCH_API_V2_BASE_URL: &str = "https://api.itch.io";

const ITCH_API_ERROR_AUTHENTICATION_REQUIRED: &str = "authentication required";
const ITCH_API_ERROR_INVALID_API_KEY: &str = "invalid key";
const ITCH_API_ERROR_INVALID_USER_OR_PASSWORD: &str = "Incorrect username or password";
const ITCH_API_ERROR_INVALID_CAPTCHA_CODE: &str = "Please correctly complete reCAPTCHA";
const ITCH_API_ERROR_INVALID_TOTP_CODE: &str = "invalid code";
const ITCH_API_ERROR_INVALID_GAME: &str = "invalid game";
const ITCH_API_ERROR_INVALID_UPLOAD: &str = "invalid upload";
const ITCH_API_ERROR_INVALID_COLLECTION: &str = "invalid collection";

/// Deserialize an empty object as an empty vector
/// 
/// This is needed because of how the itch.io API works
/// 
/// https://itchapi.ryhn.link/API/index.html
/// 
/// https://github.com/itchio/itch.io/issues/1301
pub fn empty_object_as_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error> where
  D: serde::de::Deserializer<'de>,
  T: Deserialize<'de>,
{
  struct Helper<T>(std::marker::PhantomData<T>);

  impl<'de, T> serde::de::Visitor<'de> for Helper<T> where
    T: Deserialize<'de>,
  {
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
      formatter.write_str("an array or an empty object")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Vec<T>, A::Error> where
      A: serde::de::SeqAccess<'de>,
    {
      let mut items = Vec::new();
      while let Some(item) = seq.next_element()? {
        items.push(item);
      }
      Ok(items)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Vec<T>, A::Error> where
      A: serde::de::MapAccess<'de>,
    {
      // Consume all keys without using them, returning empty Vec
      while let Some((_k, _v)) = map.next_entry::<serde::de::IgnoredAny, serde::de::IgnoredAny>()? {
        // Just ignore
      }
      Ok(vec![])
    }
  }

  deserializer.deserialize_any(Helper(std::marker::PhantomData))
}

/// A itch.io API address
/// 
/// Use the Other variant with the full URL when it isn't a known API version
pub enum ItchApiUrl<'a> {
  V1(&'a str),
  V2(&'a str),
  Other(&'a str),
}

impl std::fmt::Display for ItchApiUrl<'_> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", 
      match self {
        ItchApiUrl::V1(u) => format!("{ITCH_API_V1_BASE_URL}/{u}"),
        ItchApiUrl::V2(u) => format!("{ITCH_API_V2_BASE_URL}/{u}"),
        ItchApiUrl::Other(u) => u.to_string(),
      }
    )
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GameClassification {
  #[serde(rename = "game")]
  Game,
  #[serde(rename = "assets")]
  Assets,
  #[serde(rename = "game_mod")]
  GameMod,
  #[serde(rename = "physical_game")]
  PhysicalGame,
  #[serde(rename = "soundtrack")]
  Soundtrack,
  #[serde(rename = "tool")]
  Tool,
  #[serde(rename = "comic")]
  Comic,
  #[serde(rename = "book")]
  Book,
  #[serde(rename = "other")]
  Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GameTrait {
  #[serde(rename = "p_linux")]
  PLinux,
  #[serde(rename = "p_windows")]
  PWindows,
  #[serde(rename = "p_osx")]
  POSX,
  #[serde(rename = "p_android")]
  PAndroid,
  #[serde(rename = "can_be_bought")]
  CanBeBought,
  #[serde(rename = "has_demo")]
  HasDemo,
  #[serde(rename = "in_press_system")]
  InPressSystem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UploadTrait {
  #[serde(rename = "p_linux")]
  PLinux,
  #[serde(rename = "p_windows")]
  PWindows,
  #[serde(rename = "p_osx")]
  POSX,
  #[serde(rename = "p_android")]
  PAndroid,
  #[serde(rename = "demo")]
  Demo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GameType {
  #[serde(rename = "default")]
  Default,
  #[serde(rename = "html")]
  HTML,
  #[serde(rename = "flash")]
  Flash,
  #[serde(rename = "java")]
  Java,
  #[serde(rename = "unity")]
  Unity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UploadType {
  #[serde(rename = "default")]
  Default,
  #[serde(rename = "html")]
  HTML,
  #[serde(rename = "flash")]
  Flash,
  #[serde(rename = "java")]
  Java,
  #[serde(rename = "unity")]
  Unity,
  #[serde(rename = "soundtrack")]
  Soundtrack,
  #[serde(rename = "book")]
  Book,
  #[serde(rename = "video")]
  Video,
  #[serde(rename = "documentation")]
  Documentation,
  #[serde(rename = "mod")]
  Mod,
  #[serde(rename = "audio_assets")]
  AudioAssets,
  #[serde(rename = "graphical_assets")]
  GraphicalAssets,
  #[serde(rename = "sourcecode")]
  Sourcecode,
  #[serde(rename = "other")]
  Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
  pub id: u64,
  pub username: String,
  pub display_name: Option<String>,
  pub url: String,
  pub cover_url: Option<String>,
  pub still_cover_url: Option<String>,
  pub press_user: Option<bool>,
  pub developer: Option<bool>,
  pub gamer: Option<bool>,
}

impl User {
  pub fn get_name(&self) -> &str {
    self.display_name.as_deref().unwrap_or(self.username.as_str())
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Game {
  pub id: u64,
  pub url: String,
  pub title: String,
  pub short_text: Option<String>,
  pub r#type: GameType,
  pub classification: GameClassification,
  pub cover_url: Option<String>,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339::option", default)]
  pub published_at: Option<OffsetDateTime>,
  pub min_price: u64,
  pub user: User,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub traits: Vec<GameTrait>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Upload {
  pub position: u64,
  pub id: u64,
  pub game_id: u64,
  pub size: Option<u64>,
  pub r#type: UploadType,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub traits: Vec<UploadTrait>,
  pub filename: String,
  pub display_name: Option<String>,
  pub storage: String,
  pub host: Option<String>,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339")]
  pub updated_at: OffsetDateTime,
  pub md5_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
  pub id: u64,
  pub title: String,
  pub games_count: u64,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339")]
  pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionGameItem {
  pub game: CollectionGame,
  pub position: u64,
  pub user_id: u64,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionGame {
  pub id: u64,
  pub url: String,
  pub title: String,
  pub short_text: Option<String>,
  pub r#type: GameType,
  pub classification: GameClassification,
  pub cover_url: Option<String>,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339::option", default)]
  pub published_at: Option<OffsetDateTime>,
  pub min_price: u64,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub traits: Vec<GameTrait>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnedKey {
  pub id: u64,
  pub game_id: u64,
  pub downloads: u64,
  pub game: Game,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339")]
  pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatedGame {
  pub id: u64,
  pub url: String,
  pub title: String,
  pub short_text: Option<String>,
  pub r#type: GameType,
  pub classification: GameClassification,
  pub cover_url: Option<String>,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339::option", default)]
  pub published_at: Option<OffsetDateTime>,
  pub min_price: u64,
  pub user: User,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub traits: Vec<GameTrait>,
  pub views_count: u64,
  pub purchases_count: u64,
  pub downloads_count: u64,
  pub published: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItchCookie {
  pub itchio: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItchKey {
  pub key: String,
  pub id: u64,
  pub user_id: u64,
  pub source: String,
  pub revoked: Option<bool>,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339")]
  pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ApiResponse<T> {
  Success(T),
  Error {
    #[serde(deserialize_with = "empty_object_as_vec")]
    errors: Vec<String>,
  },
}

#[derive(Error, Debug)]
pub enum ApiResponseError {
  #[error("An API key is required in order to send any API request.")]
  AuthenticationRequired,

  #[error("The provided API key is invalid!")]
  InvalidApiKey,

  #[error("The itch.io API returned an error:\n{0:#?}")]
  Other(Vec<String>),
}

impl From<Vec<String>> for ApiResponseError {
  fn from(value: Vec<String>) -> Self {
    match value.as_slice() {
      [v] if v == ITCH_API_ERROR_AUTHENTICATION_REQUIRED => Self::AuthenticationRequired,
      [v] if v == ITCH_API_ERROR_INVALID_API_KEY => Self::InvalidApiKey,
      _ => Self::Other(value),
    }
  }
}

pub trait IntoApiResult {
  type Ok;
  type Err: std::fmt::Debug + std::error::Error;

  fn into_result(self) -> Result<Self::Ok, Self::Err>;
}

pub trait ApiResultWithoutCustomErrors {}

impl<T: ApiResultWithoutCustomErrors> IntoApiResult for ApiResponse<T> {
  type Ok = T;
  type Err = ApiResponseError;

  fn into_result(self) -> Result<Self::Ok, Self::Err> {
    match self {
      ApiResponse::Success(val) => Ok(val),
      ApiResponse::Error { errors } => Err(errors.into()),
    }
  }
}

/// Response struct for:
/// 
/// https://api.itch.io/login
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LoginResponse {
  Success(LoginSuccess),
  CaptchaError(LoginCaptchaError),
  TOTPError(LoginTOTPError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoginSuccess {
  pub success: bool,
  pub cookie: ItchCookie,
  pub key: ItchKey,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoginCaptchaError {
  pub success: bool,
  pub recaptcha_needed: bool,
  pub recaptcha_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoginTOTPError {
  pub success: bool,
  pub totp_needed: bool,
  pub token: String,
}

#[derive(Error, Debug)]
pub enum LoginResponseError {
  #[error("The username or the password is incorrect.")]
  IncorrectUsernameOrPassword,

  #[error(
r#"A reCAPTCHA verification is required to continue!
  Go to "{}" and solve the reCAPTCHA.
  To obtain the token, paste the following command on the developer console:
    console.log(grecaptcha.getResponse())
  Then run the login command again with the --recaptcha-response option."#,
  .0.recaptcha_url,
)]
  CaptchaNeeded(LoginCaptchaError),

  #[error("The reCAPTCHA response code is incorrect!")]
  IncorrectCaptchaResponseCode,

  #[error(
r#"The accout has 2 step verification enabled via TOTP
  Run the login command again with the --totp-code={{VERIFICATION_CODE}} option."#
)]
  TOTPNeeded(LoginTOTPError),

  #[error("The TOTP code is incorrect!")]
  IncorrectTOTPCode,
  
  #[error(transparent)]
  Other(#[from] ApiResponseError),
}

impl IntoApiResult for ApiResponse<LoginResponse> {
  type Ok = LoginSuccess;
  type Err = LoginResponseError;

  fn into_result(self) -> Result<Self::Ok, Self::Err> {
    match self {
      ApiResponse::Success(lr) => match lr {
        LoginResponse::Success(ls) => Ok(ls),
        LoginResponse::CaptchaError(lce) => Err(LoginResponseError::CaptchaNeeded(lce)),
        LoginResponse::TOTPError(lte) => Err(LoginResponseError::TOTPNeeded(lte)),
      }
      ApiResponse::Error { errors } => match errors.as_slice() {
        [s] if s == ITCH_API_ERROR_INVALID_USER_OR_PASSWORD => Err(LoginResponseError::IncorrectUsernameOrPassword),
        [s] if s == ITCH_API_ERROR_INVALID_CAPTCHA_CODE => Err(LoginResponseError::IncorrectCaptchaResponseCode),
        [s] if s == ITCH_API_ERROR_INVALID_TOTP_CODE => Err(LoginResponseError::IncorrectTOTPCode),
        _ => Err(ApiResponseError::from(errors).into()),
      }
    }
  }
}

/// Response struct for:
/// 
/// https://api.itch.io/profile
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileResponse {
  pub user: User,
}

impl ApiResultWithoutCustomErrors for ProfileResponse {}

/// Response struct for:
/// 
/// https://api.itch.io/profile/owned-keys
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnedKeysResponse {
  pub page: u64,
  pub per_page: u64,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub owned_keys: Vec<OwnedKey>,
}

impl ApiResultWithoutCustomErrors for OwnedKeysResponse {}

/// Response struct for:
/// 
/// https://api.itch.io/profile/games
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatedGamesResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub games: Vec<CreatedGame>,
}

impl ApiResultWithoutCustomErrors for CreatedGamesResponse {}

/// Response struct for:
/// 
/// https://api.itch.io/profile/collections
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionsResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub collections: Vec<Collection>,
}

impl ApiResultWithoutCustomErrors for CollectionsResponse {}

/// Response struct for:
/// 
/// https://api.itch.io/games/{GAME_ID}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameInfoResponse {
  pub game: Game,
}

#[derive(Error, Debug)]
pub enum GameInfoResponseError {
  #[error("The provided game ID is invalid.")]
  InvalidGameID,
  
  #[error(transparent)]
  Other(#[from] ApiResponseError),
}

impl IntoApiResult for ApiResponse<GameInfoResponse> {
  type Ok = GameInfoResponse;
  type Err = GameInfoResponseError;

  fn into_result(self) -> Result<Self::Ok, Self::Err> {
    match self {
      ApiResponse::Success(gur) => Ok(gur),
      ApiResponse::Error { errors } => match errors.as_slice() {
        [s] if s == ITCH_API_ERROR_INVALID_GAME => Err(GameInfoResponseError::InvalidGameID),
        _ => Err(ApiResponseError::from(errors).into())
      },
    }
  }
}

/// Response struct for:
/// 
/// https://api.itch.io/games/{GAME_ID}/uploads
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameUploadsResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub uploads: Vec<Upload>,
}

impl IntoApiResult for ApiResponse<GameUploadsResponse> {
  type Ok = GameUploadsResponse;
  type Err = GameInfoResponseError;

  fn into_result(self) -> Result<Self::Ok, Self::Err> {
    match self {
      ApiResponse::Success(gur) => Ok(gur),
      ApiResponse::Error { errors } => match errors.as_slice() {
        [s] if s == ITCH_API_ERROR_INVALID_GAME => Err(GameInfoResponseError::InvalidGameID),
        _ => Err(ApiResponseError::from(errors).into())
      },
    }
  }
}

/// Response struct for:
/// 
/// https://api.itch.io/uploads/{UPLOAD_ID}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadResponse {
  pub upload: Upload,
}

#[derive(Error, Debug)]
pub enum UploadResponseError {
  #[error("The provided upload ID is invalid.")]
  InvalidUploadID,
  
  #[error(transparent)]
  Other(#[from] ApiResponseError),
}

impl IntoApiResult for ApiResponse<UploadResponse> {
  type Ok = UploadResponse;
  type Err = UploadResponseError;

  fn into_result(self) -> Result<Self::Ok, Self::Err> {
    match self {
      ApiResponse::Success(ur) => Ok(ur),
      ApiResponse::Error { errors } => match errors.as_slice() {
        [s] if s == ITCH_API_ERROR_INVALID_UPLOAD => Err(UploadResponseError::InvalidUploadID),
        _ => Err(ApiResponseError::from(errors).into())
      },
    }
  }
}

/// Response struct for:
/// 
/// https://api.itch.io/collections/{COLLECTION_ID}/collection-games
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionGamesResponse {
  pub page: u64,
  pub per_page: u64,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub collection_games: Vec<CollectionGameItem>,
}

#[derive(Error, Debug)]
pub enum CollectionGamesResponseError {
  #[error("The provided collection ID is invalid.")]
  InvalidCollectionID,
  
  #[error(transparent)]
  Other(#[from] ApiResponseError),
}

impl IntoApiResult for ApiResponse<CollectionGamesResponse> {
  type Ok = CollectionGamesResponse;
  type Err = CollectionGamesResponseError;

  fn into_result(self) -> Result<Self::Ok, Self::Err> {
    match self {
      ApiResponse::Success(cgr) => Ok(cgr),
      ApiResponse::Error { errors } => {
        match errors.as_slice() {
          [s] if s == ITCH_API_ERROR_INVALID_COLLECTION => Err(CollectionGamesResponseError::InvalidCollectionID),
          _ => Err(ApiResponseError::from(errors).into()),
        }
      }
    }
  }
}
