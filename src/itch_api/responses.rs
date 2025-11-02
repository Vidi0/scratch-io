use super::errors::*;
use super::types::*;

use serde::{Deserialize, Serialize};

/// The itch.io API can respond with either the requested structure or a list of errors
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ApiResponse<T> {
  Success(T),
  Error {
    #[serde(deserialize_with = "empty_object_as_vec")]
    errors: Vec<String>,
  },
}

pub trait IntoResponseResult {
  type Err: From<ApiResponseError> + std::error::Error + std::fmt::Debug;
}

impl<T: IntoResponseResult> ApiResponse<T> {
  pub fn into_result(self) -> Result<T, <T as IntoResponseResult>::Err> {
    match self {
      Self::Success(v) => Ok(v),
      Self::Error { errors } => Err(ApiResponseError::from(errors).into()),
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

/// Response struct for: <https://api.itch.io/login>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LoginResponse {
  Success(LoginSuccess),
  CaptchaError(LoginCaptchaError),
  TOTPError(LoginTOTPError),
}

impl IntoResponseResult for LoginResponse {
  type Err = LoginResponseError;
}

/// Response struct for: <https://api.itch.io/totp/verify>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TOTPResponse {
  #[serde(flatten)]
  pub success: LoginSuccess,
}

impl IntoResponseResult for TOTPResponse {
  type Err = TOTPResponseError;
}

/// Response struct for: <https://api.itch.io/users/{user_id}>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserInfoResponse {
  pub user: User,
}

impl IntoResponseResult for UserInfoResponse {
  type Err = UserResponseError;
}

/// Response struct for: <https://api.itch.io/profile>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileInfoResponse {
  pub user: Profile,
}

impl IntoResponseResult for ProfileInfoResponse {
  type Err = ApiResponseCommonErrors;
}

/// Response struct for: <https://api.itch.io/profile/games>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatedGamesResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub games: Vec<CreatedGame>,
}

impl IntoResponseResult for CreatedGamesResponse {
  type Err = ApiResponseCommonErrors;
}

/// Response struct for: <https://api.itch.io/profile/owned-keys>
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

impl IntoResponseResult for ApiResponseList<OwnedKeysResponse> {
  type Err = ApiResponseCommonErrors;
}

/// Response struct for: <https://api.itch.io/profile/collections>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileCollectionsResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub collections: Vec<Collection>,
}

impl IntoResponseResult for ProfileCollectionsResponse {
  type Err = ApiResponseCommonErrors;
}

/// Response struct for: <https://api.itch.io/collections/{collection_id}>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionInfoResponse {
  pub collection: Collection,
}

impl IntoResponseResult for CollectionInfoResponse {
  type Err = CollectionResponseError;
}

/// Response struct for: <https://api.itch.io/collections/{collection_id}/collection-games>
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

impl IntoResponseResult for ApiResponseList<CollectionGamesResponse> {
  type Err = CollectionResponseError;
}

/// Response struct for: <https://api.itch.io/games/{game_id}>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameInfoResponse {
  pub game: Game,
}

impl IntoResponseResult for GameInfoResponse {
  type Err = GameResponseError;
}

/// Response struct for: <https://api.itch.io/games/{game_id}/uploads>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameUploadsResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub uploads: Vec<Upload>,
}

impl IntoResponseResult for GameUploadsResponse {
  type Err = GameResponseError;
}

/// Response struct for: <https://api.itch.io/uploads/{upload_id}>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadInfoResponse {
  pub upload: Upload,
}

impl IntoResponseResult for UploadInfoResponse {
  type Err = UploadResponseError;
}

/// Response struct for: <https://api.itch.io/uploads/{upload_id}/builds>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadBuildsResponse {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub builds: Vec<UploadBuild>,
}

impl IntoResponseResult for UploadBuildsResponse {
  type Err = UploadResponseError;
}

/// Response struct for: <https://api.itch.io/builds/{build_id}>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildInfoResponse {
  pub build: Build,
}

impl IntoResponseResult for BuildInfoResponse {
  type Err = BuildResponseError;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildUpgradePathResponseBuilds {
  #[serde(deserialize_with = "empty_object_as_vec")]
  pub builds: Vec<UpgradePathBuild>,
}

/// Response struct for: <https://api.itch.io/builds/{current_build_id}/upgrade-paths/{target_build_id}>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildUpgradePathResponse {
  pub upgrade_path: BuildUpgradePathResponseBuilds,
}

impl IntoResponseResult for BuildUpgradePathResponse {
  type Err = UpgradePathResponseError;
}
