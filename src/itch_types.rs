use serde::{Deserialize, Serialize};
use serde::de::{Deserializer, Visitor, SeqAccess};
use std::marker::PhantomData;
use std::fmt;

const ITCH_API_V1_BASE_URL: &str = "https://itch.io/api/1";
const ITCH_API_V2_BASE_URL: &str = "https://api.itch.io";

pub enum ItchApiUrl {
  V1(String),
  V2(String),
}

impl fmt::Display for ItchApiUrl {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", 
      match self {
        ItchApiUrl::V1(u) => format!("{ITCH_API_V1_BASE_URL}/{u}"),
        ItchApiUrl::V2(u) => format!("{ITCH_API_V2_BASE_URL}/{u}"),
      }
    )
  }
}

pub struct UploadArchive {
  file: std::path::PathBuf,
  format: UploadArchiveFormat,
}

pub enum UploadArchiveFormat {
  Zip(),
  Other(),
}

impl UploadArchive {
  /// Gets the archive format of the file
  /// 
  /// If the file is not an archive, then the format is `UploadArchiveFormat::Other`
  pub fn from_file(file: &std::path::Path) -> Self {
    let Some(ext) = file.extension().map(|e| e.to_string_lossy()) else {
      return UploadArchive { file: file.to_path_buf(), format: UploadArchiveFormat::Other() }
    };

    let format = if ext.eq_ignore_ascii_case("zip") {
      UploadArchiveFormat::Zip()
    } else {
      UploadArchiveFormat::Other()
    };

    UploadArchive { file: file.to_path_buf(), format }
  }

  async fn remove(&self) -> Result<(), String> {
    tokio::fs::remove_file(&self.file).await
      .map_err(|e| e.to_string())
  }

  /// Extracts the archive into a folder with the same name (without the extension)
  /// 
  /// This function can return a path to a file (if it's not a valid archive) or to the extracted folder
  pub async fn extract(self) -> Result<std::path::PathBuf, String> {
    if let UploadArchiveFormat::Other() = self.format {
      return Ok(self.file);
    }

    let file = std::fs::File::open(&self.file)
      .map_err(|e| e.to_string())?;

    let folder = self.file
      .parent()
      .unwrap()
      .join(&self.file.file_stem().expect("Empty filename?"));

    match self.format {
      UploadArchiveFormat::Other() => (),
      UploadArchiveFormat::Zip() => {
        let mut archive = zip::ZipArchive::new(&file)
          .map_err(|e| e.to_string())?;

        archive.extract_unwrapped_root_dir(&folder, zip::read::root_dir_common_filter)
          .map_err(|e| e.to_string())?;
      }
    }

    self.remove().await?;
    Ok(folder)
  }
}

fn empty_object_as_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error> where
  D: Deserializer<'de>,
  T: Deserialize<'de>,
{
  struct Helper<T>(PhantomData<T>);

  impl<'de, T> Visitor<'de> for Helper<T> where
    T: Deserialize<'de>,
  {
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
      formatter.write_str("an array or an empty object")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Vec<T>, A::Error> where
      A: SeqAccess<'de>,
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

  deserializer.deserialize_any(Helper(PhantomData))
}

#[derive(Deserialize, Serialize)]
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

#[derive(Deserialize, Serialize)]
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

#[derive(Deserialize, Serialize)]
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

#[derive(Deserialize, Serialize)]
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

#[derive(Deserialize, Serialize)]
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

#[derive(Deserialize)]
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
      self.display_name.clone().unwrap_or(String::new()),
      self.url,
      self.cover_url.clone().unwrap_or(String::new())
    )
  }
}

#[derive(Deserialize)]
pub struct Game {
  pub id: u64,
  pub url: String,
  pub title: String,
  pub short_text: Option<String>,
  pub r#type: GameType,
  pub classification: GameClassification,
  pub cover_url: Option<String>,
  pub created_at: String,
  pub published_at: Option<String>,
  pub min_price: Option<u64>,
  pub user: Option<User>,
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
  Price: {}
  Classification: {}
  Type: {}
  Published at: {}
  Created at: {}
  Traits: {}",
      self.id,
      self.title,
      self.short_text.clone().unwrap_or(String::new()),
      self.url,
      self.cover_url.clone().unwrap_or(String::new()),
      match self.min_price {
        None => String::new(),
        Some(p) => {
          if p <= 0 { String::from("Free") } else { String::from("Paid") }
        }
      },
      self.classification,
      self.r#type,
      self.created_at,
      self.published_at.clone().unwrap_or(String::new()),
      self.traits.iter().map(|t| t.to_string()).collect::<Vec<String>>().join(", ")
    )
  }
}

#[derive(Deserialize)]
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
  pub created_at: String,
  pub updated_at: Option<String>,
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
      self.size.clone().map(|n| n.to_string()).unwrap_or(String::new()),
      self.r#type,
      self.filename,
      self.display_name.clone().unwrap_or(String::new()),
      self.storage,
      self.created_at,
      self.updated_at.clone().unwrap_or(String::new()),
      self.md5_hash.clone().unwrap_or(String::new()),
      self.traits.iter().map(|t| t.to_string()).collect::<Vec<String>>().join(", ")
    )
  }
}

#[derive(Deserialize)]
pub struct Collection {
  pub id: u64,
  pub title: String,
  pub games_count: u64,
  pub created_at: String,
  pub updated_at: String,
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
      self.created_at,
      self.updated_at
    )
  }
}

#[derive(Deserialize)]
pub struct CollectionGame {
  pub game: Game,
  pub position: u64,
  pub user_id: u64,
  pub created_at: String,
}

impl fmt::Display for CollectionGame {
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
  pub collection_games: Vec<CollectionGame>,
}
