use serde::{Deserialize, Serialize};
use serde_with::{DefaultOnError, serde_as};
use time::{OffsetDateTime, serde::rfc3339};

const ITCH_API_V1_BASE_URL: &str = "https://itch.io/api/1";
const ITCH_API_V2_BASE_URL: &str = "https://api.itch.io";

pub type UserID = u64;
pub type CollectionID = u64;
pub type GameID = u64;
pub type UploadID = u64;
pub type BuildID = u64;
pub type ItchKeyID = u64;
pub type OwnedKeyID = u64;

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

/// A itch.io API address
///
/// Use the Other variant with the full URL when it isn't a known API version
#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum ItchApiUrl<'a> {
  V1(&'a str),
  V2(&'a str),
  Other(&'a str),
}

impl std::fmt::Display for ItchApiUrl<'_> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "{}",
      match self {
        ItchApiUrl::V1(u) => format!("{ITCH_API_V1_BASE_URL}/{u}"),
        ItchApiUrl::V2(u) => format!("{ITCH_API_V2_BASE_URL}/{u}"),
        ItchApiUrl::Other(u) => (*u).to_string(),
      }
    )
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItchCookie {
  pub itchio: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItchKeySource {
  Desktop,
  Android,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
  #[serde(flatten)]
  pub user: User,
  pub gamer: bool,
  pub developer: bool,
  pub press_user: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameType {
  Default,
  Html,
  Flash,
  Java,
  Unity,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub traits: Vec<GameTrait>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Game {
  #[serde(flatten)]
  pub game_info: GameCommon,
  pub user: User,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
  pub id: CollectionID,
  pub title: String,
  pub games_count: u64,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339")]
  pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionGame {
  #[serde(flatten)]
  pub game_info: GameCommon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionGameItem {
  pub game: CollectionGame,
  pub position: u64,
  pub user_id: UserID,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatedGame {
  #[serde(flatten)]
  pub game_info: GameCommon,
  pub user: User,
  pub views_count: u64,
  pub purchases_count: u64,
  pub downloads_count: u64,
  pub published: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildCommon {
  pub id: BuildID,
  #[serde_as(deserialize_as = "DefaultOnError<Option<_>>")]
  pub parent_build_id: Option<BuildID>,
  pub version: u64,
  pub user_version: String,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339")]
  pub updated_at: OffsetDateTime,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildFileType {
  Archive,
  Patch,
  Signature,
  Manifest,
  Unpacked,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildFileSubtype {
  Default,
  Optimized,
  Accelerated,
  Gzip,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildFileState {
  Uploaded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildFile {
  pub size: u64,
  pub r#type: BuildFileType,
  pub sub_type: BuildFileSubtype,
  pub state: BuildFileState,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildState {
  Completed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Build {
  #[serde(flatten)]
  pub build_info: BuildCommon,
  pub upload_id: UploadID,
  pub user: User,
  pub state: BuildState,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub files: Vec<BuildFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpgradePathBuild {
  #[serde(flatten)]
  pub build_info: BuildCommon,
  pub upload_id: UploadID,
  pub files: Vec<BuildFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadBuild {
  #[serde(flatten)]
  pub build_info: BuildCommon,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadTrait {
  PLinux,
  PWindows,
  POsx,
  PAndroid,
  Demo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
