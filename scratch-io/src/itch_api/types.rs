use serde::{Deserialize, Serialize};
use serde_with::{DefaultOnError, serde_as};
use thiserror::Error;
use time::{OffsetDateTime, serde::rfc3339};

const ITCH_API_V1_BASE_URL: &str = "https://itch.io/api/1/";
const ITCH_API_V2_BASE_URL: &str = "https://api.itch.io/";

pub type UserID = u64;
pub type CollectionID = u64;
pub type GameID = u64;
pub type UploadID = u64;
pub type BuildID = u64;
pub type ItchKeyID = u64;
pub type OwnedKeyID = u64;
pub type SaleID = u64;

/// Deserialize an empty object as an empty vector
///
/// This is needed because of how the itch.io API works
///
/// <https://itchapi.ryhn.link/API/index.html>
///
/// <https://github.com/itchio/itch.io/issues/1301>
///
/// # Errors
///
/// If deserializing the Vector fails
pub(super) fn empty_object_as_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
  D: serde::de::Deserializer<'de>,
  T: Deserialize<'de>,
{
  struct Helper<T>(std::marker::PhantomData<T>);

  impl<'de, T> serde::de::Visitor<'de> for Helper<T>
  where
    T: Deserialize<'de>,
  {
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
      formatter.write_str("an array or an empty object")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Vec<T>, A::Error>
    where
      A: serde::de::SeqAccess<'de>,
    {
      let mut items = Vec::new();
      while let Some(item) = seq.next_element()? {
        items.push(item);
      }
      Ok(items)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Vec<T>, A::Error>
    where
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

/// An itch.io API version
///
/// Its possible values are:
///
/// * `V1` - itch.io JSON API V1 <https://itch.io/api/1/>
///
/// * `V2` - itch.io JSON API V2 <https://api.itch.io/>
///
/// * `Other` - Any other URL
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItchApiVersion {
  V1,
  V2,
  Other,
}

/// An itch.io API address
///
/// Use the Other variant with the full URL when it isn't a known API version
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItchApiUrl {
  version: ItchApiVersion,
  url: String,
}

impl<'a> ItchApiUrl {
  /// Creates an [`ItchApiUrl`] by combining the API version with an endpoint path
  /// V1 and V2 prepend their base URLs
  ///
  /// Other uses the endpoint as-is
  pub fn from_api_endpoint(
    version: ItchApiVersion,
    endpoint: impl Into<std::borrow::Cow<'a, str>>,
  ) -> Self {
    let endpoint = endpoint.into();
    Self {
      version,
      url: match version {
        ItchApiVersion::V1 => format!("{ITCH_API_V1_BASE_URL}{endpoint}"),
        ItchApiVersion::V2 => format!("{ITCH_API_V2_BASE_URL}{endpoint}"),
        ItchApiVersion::Other => endpoint.into_owned(),
      },
    }
  }

  /// Returns the API version of this [`ItchApiUrl`]
  #[must_use]
  pub const fn get_version(&self) -> ItchApiVersion {
    self.version
  }
}

impl ItchApiUrl {
  /// Get a reference to the full URL string
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.url
  }
}

impl std::fmt::Display for ItchApiUrl {
  /// Format the [`ItchApiUrl`] as a string, returning the full URL
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.url)
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItchCookie {
  pub itchio: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItchKeySource {
  Desktop,
  Android,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItchKey {
  pub key: String,
  pub id: ItchKeyID,
  pub user_id: UserID,
  pub source: ItchKeySource,
  pub revoked: Option<bool>,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339")]
  pub updated_at: OffsetDateTime,
  #[serde(with = "rfc3339::option", default)]
  pub last_used_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginSuccess {
  pub success: bool,
  pub cookie: ItchCookie,
  pub key: ItchKey,
}

// LoginCaptchaError is defined here because it's not returned by the API
// the same way the other errors, but in its own separate struct
#[derive(Error, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[error(
  r#"A reCAPTCHA verification is required to continue!
  Go to "{recaptcha_url}" and solve the reCAPTCHA.
  To obtain the token, paste the following command on the developer console:
    console.log(grecaptcha.getResponse())
  Then run the login command again with the --recaptcha-response option."#
)]
pub struct LoginCaptchaError {
  pub success: bool,
  pub recaptcha_needed: bool,
  pub recaptcha_url: String,
}

// LoginTOTPError is defined here because it's not returned by the API
// the same way the other errors, but in its own separate struct
#[derive(Error, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[error(
  r#"The account has two-step verification enabled via TOTP.
  To complete the login, run the totp verification command with the following options:
    --totp-token="{token}"
    --totp-code={{VERIFICATION_CODE}}"#
)]
pub struct LoginTOTPError {
  pub success: bool,
  pub totp_needed: bool,
  pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
  pub id: UserID,
  pub username: String,
  pub display_name: Option<String>,
  pub url: String,
  pub cover_url: Option<String>,
  /// Only present if `cover_url` is animated. URL to the first frame of the cover.
  pub still_cover_url: Option<String>,
}

impl User {
  /// Get the display name of the user, or the username if it is missing
  #[must_use]
  pub fn get_name(&self) -> &str {
    self.display_name.as_deref().unwrap_or(&self.username)
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
  #[serde(flatten)]
  pub user: User,
  pub gamer: bool,
  pub developer: bool,
  pub press_user: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSale {
  pub id: SaleID,
  /// Rate must be an integger between -100 and 100
  /// A negative number means the game is more expensive than it was before the sale
  pub rate: i8,
  #[serde(with = "rfc3339")]
  pub start_date: OffsetDateTime,
  #[serde(with = "rfc3339")]
  pub end_date: OffsetDateTime,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameType {
  Default,
  Html,
  Flash,
  Java,
  Unity,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameClassification {
  Game,
  Assets,
  GameMod,
  PhysicalGame,
  Soundtrack,
  Tool,
  Comic,
  Book,
  Other,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameTrait {
  PLinux,
  PWindows,
  POsx,
  PAndroid,
  CanBeBought,
  HasDemo,
  InPressSystem,
}

/// This struct represents all the shared fields among the different Game structs
///
/// It should always be used alongside serde flattten
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameCommon {
  pub id: GameID,
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
  pub sale: Option<GameSale>,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub traits: Vec<GameTrait>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
  #[serde(flatten)]
  pub game_info: GameCommon,
  pub user: User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
  pub id: CollectionID,
  pub title: String,
  pub games_count: u64,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339")]
  pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionGame {
  #[serde(flatten)]
  pub game_info: GameCommon,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionGameItem {
  pub game: CollectionGame,
  pub position: u64,
  pub user_id: UserID,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedGame {
  #[serde(flatten)]
  pub game_info: GameCommon,
  pub user: User,
  pub views_count: u64,
  pub purchases_count: u64,
  pub downloads_count: u64,
  pub published: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedKey {
  pub id: OwnedKeyID,
  pub game_id: GameID,
  pub downloads: u64,
  pub game: Game,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339")]
  pub updated_at: OffsetDateTime,
}

/// This struct represents all the shared fields among the different Build structs
///
/// It should always be used alongside serde flattten
#[serde_as]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildCommon {
  pub id: BuildID,
  #[serde_as(deserialize_as = "DefaultOnError")]
  pub parent_build_id: Option<BuildID>,
  pub version: u64,
  pub user_version: Option<String>,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339")]
  pub updated_at: OffsetDateTime,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildFileType {
  Archive,
  Patch,
  Signature,
  Manifest,
  Unpacked,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildFileSubtype {
  Default,
  Optimized,
  Accelerated,
  Gzip,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildFileState {
  Uploaded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildFile {
  pub size: u64,
  pub r#type: BuildFileType,
  pub sub_type: BuildFileSubtype,
  pub state: BuildFileState,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildState {
  Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Build {
  #[serde(flatten)]
  pub build_info: BuildCommon,
  pub upload_id: UploadID,
  pub user: User,
  pub state: BuildState,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub files: Vec<BuildFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradePathBuild {
  #[serde(flatten)]
  pub build_info: BuildCommon,
  pub upload_id: UploadID,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub files: Vec<BuildFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadBuild {
  #[serde(flatten)]
  pub build_info: BuildCommon,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadType {
  Default,
  Html,
  Flash,
  Java,
  Unity,
  Soundtrack,
  Book,
  Video,
  Documentation,
  Mod,
  AudioAssets,
  GraphicalAssets,
  Sourcecode,
  Other,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadTrait {
  PLinux,
  PWindows,
  POsx,
  PAndroid,
  Demo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "storage", rename_all = "snake_case")]
pub enum UploadStorage {
  Hosted {
    size: u64,
    md5_hash: Option<String>,
  },
  Build {
    size: u64,
    build: UploadBuild,
    build_id: BuildID,
    channel_name: String,
  },
  External {
    host: String,
  },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upload {
  pub position: u64,
  pub id: UploadID,
  pub game_id: GameID,
  pub r#type: UploadType,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub traits: Vec<UploadTrait>,
  pub filename: String,
  pub display_name: Option<String>,
  #[serde(flatten)]
  pub storage: UploadStorage,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339")]
  pub updated_at: OffsetDateTime,
}

impl Upload {
  /// Get the display name of the upload, or the filename if it is missing
  #[must_use]
  pub fn get_name(&self) -> &str {
    self.display_name.as_deref().unwrap_or(&self.filename)
  }

  /// Get the hash of the upload, or None if it is missing
  #[must_use]
  pub fn get_hash(&self) -> Option<&str> {
    match &self.storage {
      UploadStorage::Hosted { md5_hash, .. } => md5_hash.as_deref(),
      _ => None,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestActionPlatform {
  Linux,
  Windows,
  Osx,
  Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestAction {
  pub name: String,
  pub path: String,
  pub platform: Option<ManifestActionPlatform>,
  pub args: Option<Vec<String>>,
  pub sandbox: Option<bool>,
  pub console: Option<bool>,
  /// Games can ask for an itch.io API key by setting the `scope` parameter
  pub scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManifestPrerequisiteName {
  #[serde(rename = "vcredist-2010-x64")]
  Vcredist2010x64,
  #[serde(rename = "vcredist-2010-x86")]
  Vcredist2010x86,
  #[serde(rename = "vcredist-2013-x64")]
  Vcredist2013x64,
  #[serde(rename = "vcredist-2013-x86")]
  Vcredist2013x86,
  #[serde(rename = "vcredist-2015-x64")]
  Vcredist2015x64,
  #[serde(rename = "vcredist-2015-x86")]
  Vcredist2015x86,
  #[serde(rename = "vcredist-2017-x64")]
  Vcredist2017x64,
  #[serde(rename = "vcredist-2017-x86")]
  Vcredist2017x86,
  #[serde(rename = "vcredist-2019-x64")]
  Vcredist2019x64,
  #[serde(rename = "vcredist-2019-x86")]
  Vcredist2019x86,

  #[serde(rename = "net-4.5.2")]
  Net452,
  #[serde(rename = "net-4.6")]
  Net46,
  #[serde(rename = "net-4.6.2")]
  Net462,

  #[serde(rename = "xna-4.0")]
  Xna40,

  #[serde(rename = "dx-june-2010")]
  DxJune2010,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestPrerequisite {
  pub name: ManifestPrerequisiteName,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
  pub actions: Option<Vec<ManifestAction>>,
  pub prereqs: Option<Vec<ManifestPrerequisite>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "object_type", rename_all = "snake_case")]
pub enum ScannedArchiveObject {
  Upload { object_id: UploadID },
  Build { object_id: BuildID },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScannedArchive {
  #[serde(flatten)]
  pub object_type: ScannedArchiveObject,
  pub extracted_size: Option<u64>,
  pub manifest: Option<Manifest>,
  // TODO: add launch targets structure
  //pub launch_targets: Option<Vec<>>,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339")]
  pub updated_at: OffsetDateTime,
}
