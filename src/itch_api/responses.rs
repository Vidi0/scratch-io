use super::types::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
      ApiResponse::Error { errors } => Err(format!(
        "The server replied with an error:\n{}",
        errors.join("\n")
      )),
    }
  }
}

/// This struct corresponds to the response to API calls that return
/// a list of items split in pages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiResponseList<T>
where
  T: ListResponse,
{
  pub page: u64,
  pub per_page: u64,
  #[serde(flatten)]
  pub values: T,
}

pub trait ListResponse {
  type Item;
  fn items(self) -> Vec<Self::Item>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LoginResponse {
  Success(LoginSuccess),
  CaptchaError(LoginCaptchaError),
  TOTPError(LoginTOTPError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileInfoResponse {
  pub user: User,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatedGamesResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub games: Vec<CreatedGame>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnedKeysResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub owned_keys: Vec<OwnedKey>,
}

impl ListResponse for OwnedKeysResponse {
  type Item = OwnedKey;

  fn items(self) -> Vec<Self::Item> {
    self.owned_keys
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileCollectionsResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub collections: Vec<Collection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionInfoResponse {
  pub collection: Collection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionGamesResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub collection_games: Vec<CollectionGameItem>,
}

impl ListResponse for CollectionGamesResponse {
  type Item = CollectionGameItem;

  fn items(self) -> Vec<Self::Item> {
    self.collection_games
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameInfoResponse {
  pub game: Game,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameUploadsResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub uploads: Vec<Upload>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadInfoResponse {
  pub upload: Upload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadBuildsResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub builds: Vec<UploadBuild>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildInfoResponse {
  pub build: Build,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildUpgradePathResponse {
  pub upgrade_path: BuildUpgradePathResponseBuilds,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildUpgradePathResponseBuilds {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub builds: Vec<UpgradePathBuild>,
}
