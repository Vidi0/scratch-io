use serde::{Deserialize};
use serde::de::{self, Deserializer, Visitor, SeqAccess};
use std::marker::PhantomData;

fn empty_object_as_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error> where
  D: Deserializer<'de>,
  T: Deserialize<'de>,
{
  struct Helper<T>(PhantomData<T>);

  impl<'de, T> Visitor<'de> for Helper<T> where
    T: Deserialize<'de>,
  {
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
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

fn deserialize_option_vec<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error> where
  D: Deserializer<'de>,
  T: Deserialize<'de>,
{
  struct OptVecVisitor<T>(PhantomData<T>);

  impl<'de, T> Visitor<'de> for OptVecVisitor<T> where
    T: Deserialize<'de>,
  {
    type Value = Option<Vec<T>>;

    fn expecting(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
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
#[allow(dead_code)]
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
#[allow(dead_code)]
pub struct Sale {
  pub id: u64,
  pub rate: f64,
  pub end_date: String,
  pub start_date: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct GameEmbedData {
  pub height: u64,
  pub width: u64,
  pub fullscreen: bool,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct Game {
  pub id: u64,
  pub url: String,
  pub title: String,
  pub short_text: Option<String>,
  pub r#type: String,
  pub classification: String,
  pub embed: Option<GameEmbedData>,
  pub cover_url: Option<String>,
  pub published_at: Option<String>,
  pub created_at: Option<String>,
  pub min_price: Option<u64>,
  pub user: Option<User>,
  #[serde(deserialize_with = "deserialize_option_vec")]
  pub traits: Option<Vec<String>>,
  pub sale: Option<Sale>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct GameUpload {
  pub id: u64,
  pub game_id: u64,
  pub size: u64,
  pub r#type: String,
  pub updated_at: Option<String>,
  pub created_at: Option<String>,
  pub position: u64,
  pub md5_hash: Option<String>,
  pub filename: String,
  #[serde(deserialize_with = "deserialize_option_vec")]
  pub traits: Option<Vec<String>>,
  pub storage: String,
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
pub enum GameInfoResponse {
  Success { game: Game },
  Error {
    #[serde(deserialize_with = "empty_object_as_vec")]
    errors: Vec<String>
  },
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
