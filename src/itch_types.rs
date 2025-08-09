use serde::{Deserialize};

#[derive(Deserialize)]
#[allow(dead_code)]
struct User {
  id: u64,
  username: Option<String>,
  display_name: Option<String>,
  url: Option<String>,
  cover_url: Option<String>,
  still_cover_url: Option<String>,
  press_user: Option<bool>,
  developer: Option<bool>,
  gamer: Option<bool>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Sale {
  id: u64,
  rate: f64,
  end_date: String,
  start_date: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct GameEmbedData {
  height: u64,
  width: u64,
  fullscreen: bool,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct Game {
  pub id: u64,
  pub url: String,
  pub title: String,
  pub short_text: Option<String>,
  pub r#type: Option<String>,
  pub classification: Option<String>,
  embed: Option<GameEmbedData>,
  pub cover_url: Option<String>,
  pub published_at: Option<String>,
  pub created_at: Option<String>,
  min_price: Option<u64>,
  user: Option<User>,
  traits: Option<Vec<String>>,
  sale: Option<Sale>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum GameInfoResponse {
  Success { game: Game },
  Error { errors: Vec<String> },
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum VerifyAPIKeyResponse {
  Success { r#type: String },
  Error { errors: Vec<String> },
}
