use serde::{Deserialize};
use serde::de::{self, Deserializer, Visitor, SeqAccess};
use std::marker::PhantomData;
use std::fmt;

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

#[allow(dead_code)]
fn deserialize_option_vec<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error> where
  D: Deserializer<'de>,
  T: Deserialize<'de>,
{
  struct OptVecVisitor<T>(PhantomData<T>);

  impl<'de, T> Visitor<'de> for OptVecVisitor<T> where
    T: Deserialize<'de>,
  {
    type Value = Option<Vec<T>>;

    fn expecting(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
      fmt.write_str("an optional list—or an empty object—as a fallback")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> where
      E: de::Error,
    {
      Ok(None)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> where
      E: de::Error,
    {
      Ok(None)
    }

    fn visit_some<D2>(self, deserializer: D2) -> Result<Self::Value, D2::Error> where
      D2: Deserializer<'de>,
    {
      // Reuse base logic (handling both sequences and maps)
      empty_object_as_vec(deserializer).map(Some)
    }
  }

  deserializer.deserialize_option(OptVecVisitor(PhantomData))
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

#[derive(Deserialize)]
pub struct GameEmbedData {
  pub height: u64,
  pub width: u64,
  pub fullscreen: bool,
}

#[derive(Deserialize)]
pub enum Trait {
  #[serde(rename = "p_linux")]
  PLinux,
  #[serde(rename = "p_windows")]
  PWindows,
  #[serde(rename = "p_osx")]
  POSX,
  #[serde(rename = "can_be_bought")]
  CanBeBought,
  #[serde(rename = "has_demo")]
  HasDemo,
  #[serde(rename = "in_press_system")]
  InPressSystem,
}

impl Trait {
  fn to_string(&self) -> String {
    match self {
      Trait::PLinux => "p_linux",
      Trait::PWindows => "p_windows",
      Trait::POSX => "p_osx",
      Trait::CanBeBought => "can_be_bought",
      Trait::HasDemo => "has_demo",
      Trait::InPressSystem => "in_press_system",
    }.to_string()
  }
}

impl fmt::Display for Trait {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.to_string())
  }
}

#[derive(Deserialize)]
pub enum Classification {
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

impl Classification {
  fn to_string(&self) -> String {
    match self {
      Classification::Game => "game",
      Classification::Assets => "assets",
      Classification::GameMod => "game_mod",
      Classification::PhysicalGame => "physical_game",
      Classification::Soundtrack => "soundtrack",
      Classification::Tool => "tool",
      Classification::Comic => "comic",
      Classification::Book => "book",
      Classification::Other => "other",
    }.to_string()
  }
}

impl fmt::Display for Classification {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.to_string())
  }
}

#[derive(Deserialize)]
pub enum Type {
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

impl Type {
  fn to_string(&self) -> String {
    match self {
      Type::Default => "default",
      Type::HTML => "html",
      Type::Flash => "flash",
      Type::Java => "java",
      Type::Unity => "unity",
    }.to_string()
  }
}

impl fmt::Display for Type {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.to_string())
  }
}

#[derive(Deserialize)]
pub struct Game {
  pub id: u64,
  pub url: String,
  pub title: String,
  pub short_text: Option<String>,
  pub r#type: Type,
  pub classification: Classification,
  pub embed: Option<GameEmbedData>,
  pub cover_url: Option<String>,
  pub created_at: String,
  pub published_at: Option<String>,
  pub min_price: Option<u64>,
  pub user: Option<User>,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub traits: Vec<Trait>,
}

impl Game {
  fn to_string(&self) -> String {
    format!("\
Id: {}
Game: {}
  Description:  {}
  URL:  {}
  Cover URL:  {}
  Price:  {}
  Classification: {}
  Type: {}
  Published at: {}
  Created at: {}",
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
      self.published_at.clone().unwrap_or(String::new())
    )
  }
}

impl fmt::Display for Game {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.to_string())
  }
}

#[derive(Deserialize)]
pub struct GameUpload {
  pub position: u64,
  pub id: u64,
  pub game_id: u64,
  pub size: u64,
  pub r#type: Type,
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub traits: Vec<Trait>,
  pub filename: String,
  pub storage: String,
  pub updated_at: Option<String>,
  pub created_at: Option<String>,
  pub md5_hash: Option<String>,
}

#[derive(Deserialize)]
pub struct Collection {
  pub id: u64,
  pub title: String,
  pub games_count: u64,
  pub created_at: String,
  pub updated_at: String,
}

impl Collection {
  fn to_string(&self) -> String {
    format!("\
Id: {}
Name: {}
  Games count:  {}
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

impl fmt::Display for Collection {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.to_string())
  }
}

#[derive(Deserialize)]
pub struct CollectionGame {
  pub game: Game,
  pub position: u64,
  pub user_id: u64,
  pub created_at: String,
}

impl CollectionGame {
  fn to_string(&self) -> String {
    format!("\
Position: {}
{}",
      self.position,
      self.game,
    )
  }
}

impl fmt::Display for CollectionGame {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.to_string())
  }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum VerifyAPIKeyResponse {
  Success { r#type: String },
  Error {
    #[serde(deserialize_with = "empty_object_as_vec")]
    errors: Vec<String>
  },
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum GameInfoResponse {
  Success { game: Game },
  Error {
    #[serde(deserialize_with = "empty_object_as_vec")]
    errors: Vec<String>
  },
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum GameUploadsResponse {
  Success {
    #[serde(deserialize_with = "empty_object_as_vec")]
    uploads: Vec<GameUpload>
  },
  Error {
    #[serde(deserialize_with = "empty_object_as_vec")]
    errors: Vec<String>
  },
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum CollectionsResponse {
  Success {
    #[serde(deserialize_with = "empty_object_as_vec")]
    collections: Vec<Collection>
  },
  Error {
    #[serde(deserialize_with = "empty_object_as_vec")]
    errors: Vec<String>
  },
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum CollectionGamesResponse {
  Success {
    page: u64,
    per_page: u64,
    #[serde(deserialize_with = "empty_object_as_vec")]
    collection_games: Vec<CollectionGame>,
  },
  Error {
    #[serde(deserialize_with = "empty_object_as_vec")]
    errors: Vec<String>
  },
}