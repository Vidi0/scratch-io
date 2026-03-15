use super::{ItchApiUrl, ItchClient};

use super::errors::*;
use super::responses::*;
use super::types::*;

use reqwest::Method;

/// Get a user's info
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// # Returns
///
/// A [`User`] struct with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_user_info(
  client: &ItchClient,
  user_id: UserID,
) -> Result<User, ItchRequestJSONError<UserResponseError>> {
  client
    .itch_request_json::<UserInfoResponse>(
      &ItchApiUrl::v2(&format!("users/{user_id}")),
      Method::GET,
      |b| b,
    )
    .map(|res| res.user)
}

/// Get the API key's profile
///
/// This can be used to verify that a given itch.io API key is valid
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// # Returns
///
/// A [`Profile`] struct with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_profile(
  client: &ItchClient,
) -> Result<Profile, ItchRequestJSONError<ApiResponseCommonErrors>> {
  client
    .itch_request_json::<ProfileInfoResponse>(&ItchApiUrl::v2("profile"), Method::GET, |b| b)
    .map(|res| res.user)
}

/// Get the games that the user created or that the user is an admin of
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// # Returns
///
/// A vector of [`CreatedGame`] structs with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_created_games(
  client: &ItchClient,
) -> Result<Vec<CreatedGame>, ItchRequestJSONError<ApiResponseCommonErrors>> {
  client
    .itch_request_json::<CreatedGamesResponse>(&ItchApiUrl::v2("profile/games"), Method::GET, |b| b)
    .map(|res| res.games)
}

/// Get the user's owned game keys
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// # Returns
///
/// A vector of [`OwnedKey`] structs with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_owned_keys(
  client: &ItchClient,
) -> Result<Vec<OwnedKey>, ItchRequestJSONError<ApiResponseCommonErrors>> {
  let mut values: Vec<OwnedKey> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let response = client.itch_request_json::<OwnedKeysResponse>(
      &ItchApiUrl::v2("profile/owned-keys"),
      Method::GET,
      |b| b.query(&[("page", page)]),
    )?;

    let response_values = response.owned_keys;
    let num_elements: u64 = response_values.len() as u64;
    values.extend(response_values);

    if num_elements == 0 || num_elements < response.per_page {
      break;
    }

    page += 1;
  }

  Ok(values)
}

/// List the user's game collections
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// # Returns
///
/// A vector of [`Collection`] structs with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_profile_collections(
  client: &ItchClient,
) -> Result<Vec<Collection>, ItchRequestJSONError<ApiResponseCommonErrors>> {
  client
    .itch_request_json::<ProfileCollectionsResponse>(
      &ItchApiUrl::v2("profile/collections"),
      Method::GET,
      |b| b,
    )
    .map(|res| res.collections)
}

/// Get a collection's info
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// * `collection_id` - The ID of the collection from which information will be obtained
///
/// # Returns
///
/// A [`Collection`] struct with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_collection_info(
  client: &ItchClient,
  collection_id: CollectionID,
) -> Result<Collection, ItchRequestJSONError<CollectionResponseError>> {
  client
    .itch_request_json::<CollectionInfoResponse>(
      &ItchApiUrl::v2(&format!("collections/{collection_id}")),
      Method::GET,
      |b| b,
    )
    .map(|res| res.collection)
}

/// List a collection's games
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// * `collection_id` - The ID of the collection from which information will be obtained
///
/// # Returns
///
/// A vector of [`CollectionGameItem`] structs with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_collection_games(
  client: &ItchClient,
  collection_id: CollectionID,
) -> Result<Vec<CollectionGameItem>, ItchRequestJSONError<CollectionResponseError>> {
  let mut values: Vec<CollectionGameItem> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let response = client.itch_request_json::<CollectionGamesResponse>(
      &ItchApiUrl::v2(&format!("collections/{collection_id}/collection-games")),
      Method::GET,
      |b| b.query(&[("page", page)]),
    )?;

    let response_values = response.collection_games;
    let num_elements: u64 = response_values.len() as u64;
    values.extend(response_values);

    if num_elements == 0 || num_elements < response.per_page {
      break;
    }

    page += 1;
  }

  Ok(values)
}

/// Get the information about a game in itch.io
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// * `game_id` - The ID of the game from which information will be obtained
///
/// # Returns
///
/// A [`Game`] struct with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_game_info(
  client: &ItchClient,
  game_id: GameID,
) -> Result<Game, ItchRequestJSONError<GameResponseError>> {
  client
    .itch_request_json::<GameInfoResponse>(
      &ItchApiUrl::v2(&format!("games/{game_id}")),
      Method::GET,
      |b| b,
    )
    .map(|res| res.game)
}

/// Get the game's uploads (downloadable files)
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// * `game_id` - The ID of the game from which information will be obtained
///
/// # Returns
///
/// A vector of [`Upload`] structs with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_game_uploads(
  client: &ItchClient,
  game_id: GameID,
) -> Result<Vec<Upload>, ItchRequestJSONError<GameResponseError>> {
  client
    .itch_request_json::<GameUploadsResponse>(
      &ItchApiUrl::v2(&format!("games/{game_id}/uploads")),
      Method::GET,
      |b| b,
    )
    .map(|res| res.uploads)
}

/// Get an upload's info
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// * `upload_id` - The ID of the upload from which information will be obtained
///
/// # Returns
///
/// An [`Upload`] struct with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_upload_info(
  client: &ItchClient,
  upload_id: UploadID,
) -> Result<Upload, ItchRequestJSONError<UploadResponseError>> {
  client
    .itch_request_json::<UploadInfoResponse>(
      &ItchApiUrl::v2(&format!("uploads/{upload_id}")),
      Method::GET,
      |b| b,
    )
    .map(|res| res.upload)
}

/// Get the upload's builds (downloadable versions)
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// * `upload_id` - The ID of the upload from which information will be obtained
///
/// # Returns
///
/// A vector of [`UploadBuild`] structs with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_upload_builds(
  client: &ItchClient,
  upload_id: UploadID,
) -> Result<Vec<UploadBuild>, ItchRequestJSONError<UploadResponseError>> {
  client
    .itch_request_json::<UploadBuildsResponse>(
      &ItchApiUrl::v2(&format!("uploads/{upload_id}/builds")),
      Method::GET,
      |b| b,
    )
    .map(|res| res.builds)
}

/// Get a build's info
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// * `build_id` - The ID of the build from which information will be obtained
///
/// # Returns
///
/// A [`Build`] struct with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_build_info(
  client: &ItchClient,
  build_id: BuildID,
) -> Result<Build, ItchRequestJSONError<BuildResponseError>> {
  client
    .itch_request_json::<BuildInfoResponse>(
      &ItchApiUrl::v2(&format!("builds/{build_id}")),
      Method::GET,
      |b| b,
    )
    .map(|res| res.build)
}

/// Get the upgrade path between two upload builds
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// * `current_build_id` - The ID of the current build
///
/// * `target_build_id` - The ID of the target build
///
/// # Returns
///
/// A vector of [`UpgradePathBuild`] structs with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_upgrade_path(
  client: &ItchClient,
  current_build_id: BuildID,
  target_build_id: BuildID,
) -> Result<Vec<UpgradePathBuild>, ItchRequestJSONError<UpgradePathResponseError>> {
  client
    .itch_request_json::<BuildUpgradePathResponse>(
      &ItchApiUrl::v2(&format!(
        "builds/{current_build_id}/upgrade-paths/{target_build_id}"
      )),
      Method::GET,
      |b| b,
    )
    .map(|res| res.upgrade_path.builds)
}

/// Get additional information about the contents of the upload
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// * `upload_id` - The ID of the upload from which information will be obtained
///
/// # Returns
///
/// A [`ScannedArchive`] struct with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_upload_scanned_archive(
  client: &ItchClient,
  upload_id: UploadID,
) -> Result<ScannedArchive, ItchRequestJSONError<UploadResponseError>> {
  client
    .itch_request_json::<UploadScannedArchiveResponse>(
      &ItchApiUrl::v2(&format!("uploads/{upload_id}/scanned-archive")),
      Method::GET,
      |b| b,
    )
    .map(|res| res.scanned_archive)
}

/// Get additional information about the contents of the build
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// * `build_id` - The ID of the build from which information will be obtained
///
/// # Returns
///
/// A [`ScannedArchive`] struct with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub fn get_build_scanned_archive(
  client: &ItchClient,
  build_id: BuildID,
) -> Result<ScannedArchive, ItchRequestJSONError<BuildResponseError>> {
  client
    .itch_request_json::<BuildScannedArchiveResponse>(
      &ItchApiUrl::v2(&format!("builds/{build_id}/scanned-archive")),
      Method::GET,
      |b| b,
    )
    .map(|res| res.scanned_archive)
}
