pub mod errors;
mod responses;
pub mod types;

use errors::*;
use responses::*;
pub use responses::{ApiResponse, IntoResponseResult};
use types::*;

use reqwest::{Method, Response, header};

/// A client able to send requests to the itch.io API
#[derive(Debug, Clone)]
pub struct ItchClient {
  client: reqwest::Client,
  api_key: String,
}

/// This block defiles the `ItchClient` API calls
impl ItchClient {
  /// Make a request to the itch.io API
  ///
  /// # Arguments
  ///
  /// * `url` - An itch.io API address to make the request against
  ///
  /// * `method` - The request method (GET, POST, etc.)
  ///
  /// * `options` - A closure that modifies the request builder just before sending it
  ///
  /// # Returns
  ///
  /// The reqwest `Response`
  ///
  /// # Errors
  ///
  /// If the request fails to send
  pub(crate) async fn itch_request(
    &self,
    url: ItchApiUrl<'_>,
    method: Method,
    options: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
  ) -> Result<Response, reqwest::Error> {
    // Create the base request
    let mut request: reqwest::RequestBuilder = self.client.request(method, url.to_string());

    // Add authentication based on the API's version.
    request = match url {
      // https://itchapi.ryhn.link/API/V1/index.html#authentication
      ItchApiUrl::V1(..) => {
        request.header(header::AUTHORIZATION, format!("Bearer {}", &self.api_key))
      }
      // https://itchapi.ryhn.link/API/V2/index.html#authentication
      ItchApiUrl::V2(..) => request.header(header::AUTHORIZATION, &self.api_key),
      // If it isn't a known API version, just leave it without authentication
      // Giving any authentication to an untrusted site is insecure because the API key could be stolen
      ItchApiUrl::Other(..) => request,
    };

    // This header is set to ensure the use of the v2 version
    // https://itchapi.ryhn.link/API/V2/index.html
    if let ItchApiUrl::V2(_) = url {
      request = request.header(header::ACCEPT, "application/vnd.itch.v2");
    }

    // The callback is the final option before sending because
    // it needs to be able to modify anything
    request = options(request);

    request.send().await
  }

  /// Make a request to the itch.io API and parse the response as JSON
  ///
  /// # Arguments
  ///
  /// * `url` - An itch.io API address to make the request against
  ///
  /// * `method` - The request method (GET, POST, etc.)
  ///
  /// * `options` - A closure that modifies the request builder just before sending it
  ///
  /// # Returns
  ///
  /// The JSON response parsed into the provided type
  ///
  /// # Errors
  ///
  /// If the request, retrieving its text, or parsing fails, or if the server returned an error
  async fn itch_request_json<T>(
    &self,
    url: ItchApiUrl<'_>,
    method: Method,
    options: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
  ) -> Result<T, ItchRequestJSONError<<T as IntoResponseResult>::Err>>
  where
    T: serde::de::DeserializeOwned + IntoResponseResult,
  {
    let text = self
      .itch_request(url, method, options)
      .await
      .map_err(|e| ItchRequestJSONError {
        url: url.to_string(),
        kind: ItchRequestJSONErrorKind::CouldntSend(e),
      })?
      .text()
      .await
      .map_err(|e| ItchRequestJSONError {
        url: url.to_string(),
        kind: ItchRequestJSONErrorKind::CouldntGetText(e),
      })?;

    serde_json::from_str::<ApiResponse<T>>(&text)
      .map_err(|error| ItchRequestJSONError {
        url: url.to_string(),
        kind: ItchRequestJSONErrorKind::InvalidJSON { body: text, error },
      })?
      .into_result()
      .map_err(|e| ItchRequestJSONError {
        url: url.to_string(),
        kind: ItchRequestJSONErrorKind::ServerRepliedWithError(e),
      })
  }
}

/// This block defines the `ItchClient` constructors and other functions
impl ItchClient {
  /// Obtain the API key associated with this `ItchClient`
  #[must_use]
  pub fn get_api_key(&self) -> &str {
    &self.api_key
  }

  /// Create a new client using the provided itch.io API key, without verifying its validity
  ///
  /// # Arguments
  ///
  /// * `api_key` - A valid itch.io API key to store in the client
  ///
  /// # Returns
  ///
  /// A `ItchClient` struct with the given key
  #[must_use]
  pub fn new(api_key: String) -> Self {
    Self {
      client: reqwest::Client::new(),
      api_key,
    }
  }

  /// Create a new client using the provided itch.io API key and verify its validity
  ///
  /// # Arguments
  ///
  /// * `api_key` - A valid itch.io API key to store in the client
  ///
  /// # Returns
  ///
  /// A `ItchClient` struct with the given key
  ///
  /// # Errors
  ///
  /// If the request, retrieving its text, or parsing fails, or if the server returned an error
  pub async fn auth(
    api_key: String,
  ) -> Result<Self, ItchRequestJSONError<ApiResponseCommonErrors>> {
    let client = ItchClient::new(api_key);

    // Verify that the API key is valid
    // Calling get_profile will fail if the given API key is invalid
    get_profile(&client).await?;

    Ok(client)
  }

  /// Login to itch.io
  ///
  /// Retrieve a API key from a username and password authentication
  ///
  /// # Arguments
  ///
  /// * `username` - The username OR email of the accout to log in with
  ///
  /// * `password` - The password of the accout to log in with
  ///
  /// * `recaptcha_response` - If required, the reCAPTCHA token from <https://itch.io/captcha>
  ///
  /// * `totp_code` - If required, The 6-digit code returned by the TOTP application
  ///
  /// # Returns
  ///
  /// An `ItchClient` struct with the new API key
  ///
  /// # Errors
  ///
  /// If the requests fail, or an additional step is required to log in.
  pub async fn login(
    username: &str,
    password: &str,
    recaptcha_response: Option<&str>,
    totp_code: Option<u64>,
  ) -> Result<Self, LoginError> {
    let mut client = ItchClient::new(String::new());

    let mut params: Vec<(&'static str, &str)> = vec![
      ("username", username),
      ("password", password),
      ("force_recaptcha", "false"),
      // source can be any of types::ItchKeySource
      ("source", "desktop"),
    ];

    if let Some(rr) = recaptcha_response {
      params.push(("recaptcha_response", rr));
    }

    let response = client
      .itch_request_json::<LoginResponse>(ItchApiUrl::V2("login"), Method::POST, |b| {
        b.form(&params)
      })
      .await?;

    let ls = match response {
      LoginResponse::CaptchaError(e) => return Err(LoginError::CaptchaNeeded(e)),
      LoginResponse::TOTPError(e) => {
        let Some(totp_code) = totp_code else {
          return Err(LoginError::TOTPNeeded(e));
        };

        client
          .totp_verification(&e.token, totp_code)
          .await
          .map(|res| res.success)?
      }
      LoginResponse::Success(ls) => ls,
    };

    // Save the new API key into the client for future API calls
    client.api_key = ls.key.key;

    Ok(client)
  }

  /// Complete the login with the TOTP 2nd factor verification
  ///
  /// # Arguments
  ///
  /// * `totp_token` - The TOTP token returned by the previous login step
  ///
  /// * `totp_code` - The 6-digit code returned by the TOTP application
  ///
  /// # Returns
  ///
  /// A `LoginSuccess` struct with the new API key
  ///
  /// An error if something goes wrong
  async fn totp_verification(
    &self,
    totp_token: &str,
    totp_code: u64,
  ) -> Result<TOTPResponse, ItchRequestJSONError<TOTPResponseError>> {
    self
      .itch_request_json::<TOTPResponse>(ItchApiUrl::V2("totp/verify"), Method::POST, |b| {
        b.form(&[("token", totp_token), ("code", &totp_code.to_string())])
      })
      .await
  }
}

/// Get a user's info
///
/// # Arguments
///
/// * `client` - An itch.io API client
///
/// # Returns
///
/// A `User` struct with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub async fn get_user_info(
  client: &ItchClient,
  user_id: UserID,
) -> Result<User, ItchRequestJSONError<UserResponseError>> {
  client
    .itch_request_json::<UserInfoResponse>(
      ItchApiUrl::V2(&format!("users/{user_id}")),
      Method::GET,
      |b| b,
    )
    .await
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
/// A `Profile` struct with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub async fn get_profile(
  client: &ItchClient,
) -> Result<Profile, ItchRequestJSONError<ApiResponseCommonErrors>> {
  client
    .itch_request_json::<ProfileInfoResponse>(ItchApiUrl::V2("profile"), Method::GET, |b| b)
    .await
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
/// A vector of `CreatedGame` structs with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub async fn get_created_games(
  client: &ItchClient,
) -> Result<Vec<CreatedGame>, ItchRequestJSONError<ApiResponseCommonErrors>> {
  client
    .itch_request_json::<CreatedGamesResponse>(ItchApiUrl::V2("profile/games"), Method::GET, |b| b)
    .await
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
/// A vector of `OwnedKey` structs with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub async fn get_owned_keys(
  client: &ItchClient,
) -> Result<Vec<OwnedKey>, ItchRequestJSONError<ApiResponseCommonErrors>> {
  let mut values: Vec<OwnedKey> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let response = client
      .itch_request_json::<OwnedKeysResponse>(
        ItchApiUrl::V2("profile/owned-keys"),
        Method::GET,
        |b| b.query(&[("page", page)]),
      )
      .await?;

    let response_values = response.owned_keys;
    let num_elements: u64 = response_values.len() as u64;
    values.extend(response_values.into_iter());

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
/// A vector of `Collection` structs with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub async fn get_profile_collections(
  client: &ItchClient,
) -> Result<Vec<Collection>, ItchRequestJSONError<ApiResponseCommonErrors>> {
  client
    .itch_request_json::<ProfileCollectionsResponse>(
      ItchApiUrl::V2("profile/collections"),
      Method::GET,
      |b| b,
    )
    .await
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
/// A `Collection` struct with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub async fn get_collection_info(
  client: &ItchClient,
  collection_id: CollectionID,
) -> Result<Collection, ItchRequestJSONError<CollectionResponseError>> {
  client
    .itch_request_json::<CollectionInfoResponse>(
      ItchApiUrl::V2(&format!("collections/{collection_id}")),
      Method::GET,
      |b| b,
    )
    .await
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
/// A vector of `CollectionGameItem` structs with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub async fn get_collection_games(
  client: &ItchClient,
  collection_id: CollectionID,
) -> Result<Vec<CollectionGameItem>, ItchRequestJSONError<CollectionResponseError>> {
  let mut values: Vec<CollectionGameItem> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let response = client
      .itch_request_json::<CollectionGamesResponse>(
        ItchApiUrl::V2(&format!("collections/{collection_id}/collection-games")),
        Method::GET,
        |b| b.query(&[("page", page)]),
      )
      .await?;

    let response_values = response.collection_games;
    let num_elements: u64 = response_values.len() as u64;
    values.extend(response_values.into_iter());

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
/// A `Game` struct with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub async fn get_game_info(
  client: &ItchClient,
  game_id: GameID,
) -> Result<Game, ItchRequestJSONError<GameResponseError>> {
  client
    .itch_request_json::<GameInfoResponse>(
      ItchApiUrl::V2(&format!("games/{game_id}")),
      Method::GET,
      |b| b,
    )
    .await
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
/// A vector of `Upload` structs with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub async fn get_game_uploads(
  client: &ItchClient,
  game_id: GameID,
) -> Result<Vec<Upload>, ItchRequestJSONError<GameResponseError>> {
  client
    .itch_request_json::<GameUploadsResponse>(
      ItchApiUrl::V2(&format!("games/{game_id}/uploads")),
      Method::GET,
      |b| b,
    )
    .await
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
/// A `Upload` struct with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub async fn get_upload_info(
  client: &ItchClient,
  upload_id: UploadID,
) -> Result<Upload, ItchRequestJSONError<UploadResponseError>> {
  client
    .itch_request_json::<UploadInfoResponse>(
      ItchApiUrl::V2(&format!("uploads/{upload_id}")),
      Method::GET,
      |b| b,
    )
    .await
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
/// A vector of `UploadBuild` structs with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub async fn get_upload_builds(
  client: &ItchClient,
  upload_id: UploadID,
) -> Result<Vec<UploadBuild>, ItchRequestJSONError<UploadResponseError>> {
  client
    .itch_request_json::<UploadBuildsResponse>(
      ItchApiUrl::V2(&format!("uploads/{upload_id}/builds")),
      Method::GET,
      |b| b,
    )
    .await
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
/// A `Build` struct with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub async fn get_build_info(
  client: &ItchClient,
  build_id: BuildID,
) -> Result<Build, ItchRequestJSONError<BuildResponseError>> {
  client
    .itch_request_json::<BuildInfoResponse>(
      ItchApiUrl::V2(&format!("builds/{build_id}")),
      Method::GET,
      |b| b,
    )
    .await
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
/// A vector of `UpgradePathBuild` structs with the info provided by the API
///
/// # Errors
///
/// If the request, retrieving its text, or parsing fails, or if the server returned an error
pub async fn get_upgrade_path(
  client: &ItchClient,
  current_build_id: BuildID,
  target_build_id: BuildID,
) -> Result<Vec<UpgradePathBuild>, ItchRequestJSONError<UpgradePathResponseError>> {
  client
    .itch_request_json::<BuildUpgradePathResponse>(
      ItchApiUrl::V2(&format!(
        "builds/{current_build_id}/upgrade-paths/{target_build_id}"
      )),
      Method::GET,
      |b| b,
    )
    .await
    .map(|res| res.upgrade_path.builds)
}

#[cfg(test)]
mod tests {
  use super::*;

  const INVALID_KEY: String = String::new();
  const VALID_USER_ID: UserID = 1;
  const INVALID_USER_ID: UserID = 0;
  const VALID_COLLECTION_ID: CollectionID = 4;
  const INVALID_COLLECTION_ID: CollectionID = 0;
  const VALID_GAME_ID: GameID = 3;
  const INVALID_GAME_ID: GameID = 0;
  const VALID_UPLOAD_ID: UploadID = 3;
  const INVALID_UPLOAD_ID: UploadID = 0;
  const VALID_BUILD_ID: BuildID = 117;
  const INVALID_BUILD_ID: BuildID = 0;

  /// Read the `SCRATCH_API_KEY` environment variable and create a `ItchClient` based on it
  ///
  /// # Panics
  ///
  /// If the `SCRATCH_API_KEY` environment variable isn't set
  fn get_client() -> ItchClient {
    let api_key = std::env::var("SCRATCH_API_KEY").expect("SCRATCH_API_KEY must be set for tests");
    ItchClient::new(api_key)
  }

  #[tokio::test]
  async fn test_invalid_key() {
    // This client has an empty API key
    // For that reason, it should fail with InvalidApiKey
    let client = ItchClient::auth(INVALID_KEY).await;

    assert!(matches!(
      client.unwrap_err().kind,
      ItchRequestJSONErrorKind::ServerRepliedWithError(ApiResponseCommonErrors::InvalidApiKey(_)),
    ));
  }

  #[tokio::test]
  async fn test_user_info() {
    let client = get_client();

    // Verify that retrieving the user info works
    assert!(matches!(
      get_user_info(&client, VALID_USER_ID).await.unwrap(),
      User {
        id: VALID_USER_ID,
        ..
      }
    ));

    // Verify that retrieving ther user info for an invalid user fails
    assert!(matches!(
      get_user_info(&client, INVALID_USER_ID)
        .await
        .unwrap_err()
        .kind,
      ItchRequestJSONErrorKind::ServerRepliedWithError(UserResponseError::InvalidUserID(_))
    ));
  }

  #[tokio::test]
  async fn test_profile() {
    let client = get_client();

    // Verify that retrieving the profile works
    get_profile(&client).await.unwrap();
  }

  #[tokio::test]
  async fn test_created_games() {
    let client = get_client();

    // Verify that retrieving the created games works
    get_created_games(&client).await.unwrap();
  }

  #[tokio::test]
  async fn test_owned_keys() {
    let client = get_client();

    // Verify that retrieving the owned keys works
    get_owned_keys(&client).await.unwrap();
  }

  #[tokio::test]
  async fn test_profile_collections() {
    let client = get_client();

    // Verify that retrieving the profile collections works
    get_profile_collections(&client).await.unwrap();
  }

  #[tokio::test]
  async fn test_collection_info() {
    let client = get_client();

    // Verify that retrieving the collection info works
    assert!(matches!(
      get_collection_info(&client, VALID_COLLECTION_ID)
        .await
        .unwrap(),
      Collection {
        id: VALID_COLLECTION_ID,
        ..
      }
    ));

    // Verify that retrieving the collection info for an invalid collection fails
    assert!(matches!(
      get_collection_info(&client, INVALID_COLLECTION_ID)
        .await
        .unwrap_err()
        .kind,
      ItchRequestJSONErrorKind::ServerRepliedWithError(
        CollectionResponseError::InvalidCollectionID(_)
      )
    ));
  }

  #[tokio::test]
  async fn test_collection_games() {
    let client = get_client();

    // Verify that retrieving the collection info works
    get_collection_games(&client, VALID_COLLECTION_ID)
      .await
      .unwrap();

    // Verify that retrieving the collection games for an invalid collection fails
    assert!(matches!(
      get_collection_games(&client, INVALID_COLLECTION_ID)
        .await
        .unwrap_err()
        .kind,
      ItchRequestJSONErrorKind::ServerRepliedWithError(
        CollectionResponseError::InvalidCollectionID(_)
      )
    ));
  }

  #[tokio::test]
  async fn test_game_info() {
    let client = get_client();

    // Verify that retrieving the game info works
    assert!(matches!(
      get_game_info(&client, VALID_GAME_ID)
        .await
        .unwrap()
        .game_info,
      GameCommon {
        id: VALID_GAME_ID,
        ..
      }
    ));

    // Verify that retrieving the game info for an invalid game fails
    assert!(matches!(
      get_game_info(&client, INVALID_GAME_ID)
        .await
        .unwrap_err()
        .kind,
      ItchRequestJSONErrorKind::ServerRepliedWithError(GameResponseError::InvalidGameID(_))
    ));
  }

  #[tokio::test]
  async fn test_game_uploads() {
    let client = get_client();

    // Verify that retrieving the game uploads works
    get_game_uploads(&client, VALID_GAME_ID).await.unwrap();

    // Verify that retrieving the game uploads for an invalid game fails
    assert!(matches!(
      get_game_uploads(&client, INVALID_GAME_ID)
        .await
        .unwrap_err()
        .kind,
      ItchRequestJSONErrorKind::ServerRepliedWithError(GameResponseError::InvalidGameID(_))
    ));
  }

  #[tokio::test]
  async fn test_upload_info() {
    let client = get_client();

    // Verify that retrieving the upload info works
    assert!(matches!(
      get_upload_info(&client, VALID_UPLOAD_ID).await.unwrap(),
      Upload {
        id: VALID_UPLOAD_ID,
        ..
      }
    ));

    // Verify that retrieving the upload info for an invalid upload fails
    assert!(matches!(
      get_upload_info(&client, INVALID_UPLOAD_ID)
        .await
        .unwrap_err()
        .kind,
      ItchRequestJSONErrorKind::ServerRepliedWithError(UploadResponseError::InvalidUploadID(_))
    ));
  }

  #[tokio::test]
  async fn test_upload_builds() {
    let client = get_client();

    // Verify that retrieving the upload builds works
    get_upload_builds(&client, VALID_UPLOAD_ID).await.unwrap();

    // Verify that retrieving the upload builds for an invalid upload fails
    assert!(matches!(
      get_upload_builds(&client, INVALID_UPLOAD_ID)
        .await
        .unwrap_err()
        .kind,
      ItchRequestJSONErrorKind::ServerRepliedWithError(UploadResponseError::InvalidUploadID(_))
    ));
  }

  #[tokio::test]
  async fn test_build_info() {
    let client = get_client();

    // Verify that retrieving the build info works
    assert!(matches!(
      get_build_info(&client, VALID_BUILD_ID)
        .await
        .unwrap()
        .build_info,
      BuildCommon {
        id: VALID_BUILD_ID,
        ..
      }
    ));

    // Verify that retrieving the build info for an invalid build fails
    assert!(matches!(
      get_build_info(&client, INVALID_BUILD_ID)
        .await
        .unwrap_err()
        .kind,
      ItchRequestJSONErrorKind::ServerRepliedWithError(BuildResponseError::InvalidBuildID(_))
    ));
  }
}
