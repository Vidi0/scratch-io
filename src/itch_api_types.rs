use serde::{Serialize, Deserialize};
use time::{OffsetDateTime, serde::rfc3339, format_description::well_known::Rfc3339};
use std::fmt;

const ITCH_API_V1_BASE_URL: &str = "https://itch.io/api/1";
const ITCH_API_V2_BASE_URL: &str = "https://api.itch.io";

pub fn empty_object_as_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error> where
  D: serde::de::Deserializer<'de>,
  T: Deserialize<'de>,
{
  struct Helper<T>(std::marker::PhantomData<T>);

  impl<'de, T> serde::de::Visitor<'de> for Helper<T> where
    T: Deserialize<'de>,
  {
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
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

impl fmt::Display for ItchApiUrl<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", 
      match self {
        ItchApiUrl::V1(u) => format!("{ITCH_API_V1_BASE_URL}/{u}"),
        ItchApiUrl::V2(u) => format!("{ITCH_API_V2_BASE_URL}/{u}"),
        ItchApiUrl::Other(u) => format!("{u}"),
      }
    )
  }
}

#[derive(Serialize, Deserialize)]
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

impl fmt::Display for GameTrait {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", serde_json::to_string(&self).unwrap())
  }
}

#[derive(Serialize, Deserialize)]
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

impl fmt::Display for UploadTrait {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", serde_json::to_string(&self).unwrap())
  }
}

#[derive(Serialize, Deserialize)]
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

impl fmt::Display for GameClassification {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", serde_json::to_string(&self).unwrap())
  }
}

#[derive(Serialize, Deserialize)]
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

impl fmt::Display for GameType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", serde_json::to_string(&self).unwrap())
  }
}

#[derive(Serialize, Deserialize)]
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

impl fmt::Display for UploadType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", serde_json::to_string(&self).unwrap())
  }
}

#[derive(Serialize, Deserialize)]
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

impl fmt::Display for User {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "\
Id: {}
Name: {}
Display name: {}
URL: {}
Cover URL: {}",
      self.id,
      self.username,
      self.display_name.as_deref().unwrap_or_default(),
      self.url,
      self.cover_url.as_deref().unwrap_or_default(),
    )
  }
}

#[derive(Serialize, Deserialize)]
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

impl fmt::Display for Game {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "\
Id: {}
Game: {}
  Description: {}
  URL: {}
  Cover URL: {}
  Author: {}
  Price: {}
  Classification: {}
  Type: {}
  Created at: {}
  Published at: {}
  Traits: {}",
      self.id,
      self.title,
      self.short_text.as_deref().unwrap_or_default(),
      self.url,
      self.cover_url.as_deref().unwrap_or_default(),
      self.user.get_name(),
      if self.min_price <= 0 { "Free" } else { "Paid" },
      self.classification,
      self.r#type,
      self.created_at.format(&Rfc3339).unwrap_or_default(),
      self.published_at.as_ref().and_then(|date| date.format(&Rfc3339).ok()).unwrap_or_default(),
      self.traits.iter().map(|t| t.to_string()).collect::<Vec<String>>().join(", ")
    )
  }
}

#[derive(Serialize, Deserialize)]
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

impl fmt::Display for Upload {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, 
"    Position: {}
    ID: {}
      Size: {}
      Type: {}
      Filename: {}
      Display name: {}
      Storage: {}
      Created at: {}
      Updated at: {}
      MD5 hash: {}
      Traits: {}",
      self.position,
      self.id,
      self.size.as_ref().map(|n| n.to_string()).unwrap_or_default(),
      self.r#type,
      self.filename,
      self.display_name.as_deref().unwrap_or_default(),
      self.storage,
      self.created_at.format(&Rfc3339).unwrap_or_default(),
      self.updated_at.format(&Rfc3339).unwrap_or_default(),
      self.md5_hash.as_deref().unwrap_or_default(),
      self.traits.iter().map(|t| t.to_string()).collect::<Vec<String>>().join(", ")
    )
  }
}

#[derive(Deserialize)]
pub struct Collection {
  pub id: u64,
  pub title: String,
  pub games_count: u64,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
  #[serde(with = "rfc3339")]
  pub updated_at: OffsetDateTime,
}

impl fmt::Display for Collection {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "\
Id: {}
Name: {}
  Games count: {}
  Created at: {}
  Updated at: {}",
      self.id,
      self.title,
      self.games_count,
      self.created_at.format(&Rfc3339).unwrap_or_default(),
      self.updated_at.format(&Rfc3339).unwrap_or_default(),
    )
  }
}

#[derive(Deserialize)]
pub struct CollectionGameItem {
  pub game: CollectionGame,
  pub position: u64,
  pub user_id: u64,
  #[serde(with = "rfc3339")]
  pub created_at: OffsetDateTime,
}

impl fmt::Display for CollectionGameItem {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "\
Position: {}
{}",
      self.position,
      self.game,
    )
  }
}

#[derive(Deserialize)]
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

impl fmt::Display for CollectionGame {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "\
Id: {}
Game: {}
  Description: {}
  URL: {}
  Cover URL: {}
  Price: {}
  Classification: {}
  Type: {}
  Created at: {}
  Published at: {}
  Traits: {}",
      self.id,
      self.title,
      self.short_text.as_deref().unwrap_or_default(),
      self.url,
      self.cover_url.as_deref().unwrap_or_default(),
      if self.min_price <= 0 { "Free" } else { "Paid" },
      self.classification,
      self.r#type,
      self.created_at.format(&Rfc3339).unwrap_or_default(),
      self.published_at.as_ref().and_then(|date| date.format(&Rfc3339).ok()).unwrap_or_default(),
      self.traits.iter().map(|t| t.to_string()).collect::<Vec<String>>().join(", ")
    )
  }
}

#[derive(Deserialize)]
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

impl fmt::Display for OwnedKey {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "\
Id: {}
  Game Id: {}
  Downloads: {}
  Created at: {}
  Updated at: {}",
      self.id,
      self.game_id,
      self.downloads,
      self.created_at.format(&Rfc3339).unwrap_or_default(),
      self.updated_at.format(&Rfc3339).unwrap_or_default(),
    )
  }
}

#[derive(Deserialize)]
pub struct ItchCookie {
  pub itchio: String,
}

#[derive(Deserialize)]
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

#[derive(Deserialize)]
#[serde(untagged)]
pub enum ApiResponse<T> {
  Success(T),
  Error {
    #[serde(deserialize_with = "empty_object_as_vec")]
    errors: Vec<String>,
  },
}

impl<T> ApiResponse<T> {
  pub fn into_result(self) -> Result<T, String> {
    match self {
      ApiResponse::Success(data) => Ok(data),
      ApiResponse::Error { errors } => Err(format!("The server replied with an error:\n{}", errors.join("\n"))),
    }
  }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum LoginResponse {
  Success(LoginSuccess),
  CaptchaError(LoginCaptchaError),
  TOTPError(LoginTOTPError),
}

#[derive(Deserialize)]
pub struct LoginSuccess {
  pub success: bool,
  pub cookie: ItchCookie,
  pub key: ItchKey,
}

#[derive(Deserialize)]
pub struct LoginCaptchaError {
  pub success: bool,
  pub recaptcha_needed: bool,
  pub recaptcha_url: String,
}

#[derive(Deserialize)]
pub struct LoginTOTPError {
  pub success: bool,
  pub totp_needed: bool,
  pub token: String,
}

#[derive(Deserialize)]
pub struct ProfileResponse {
  pub user: User,
}

#[derive(Deserialize)]
pub struct GameInfoResponse {
  pub game: Game,
}

#[derive(Deserialize)]
pub struct GameUploadsResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub uploads: Vec<Upload>,
}

#[derive(Deserialize)]
pub struct UploadResponse {
  pub upload: Upload,
}

#[derive(Deserialize)]
pub struct CollectionsResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub collections: Vec<Collection>,
}

#[derive(Deserialize)]
pub struct CollectionGamesResponse {
  pub page: u64,
  pub per_page: u64,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub collection_games: Vec<CollectionGameItem>,
}

#[derive(Deserialize)]
pub struct OwnedKeysResponse {
  pub page: u64,
  pub per_page: u64,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub owned_keys: Vec<OwnedKey>,
}
