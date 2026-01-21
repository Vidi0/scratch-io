pub mod errors;
mod responses;
pub mod types;

use errors::*;
use responses::*;
pub use responses::{ApiResponse, IntoResponseResult, LoginResponse};
use types::*;

use reqwest::{
  Method,
  blocking::{Client, RequestBuilder, Response},
  header,
};

/// A client able to send requests to the itch.io API
#[derive(Debug, Clone)]
pub struct ItchClient {
  client: Client,
  api_key: String,
}

/// This block defiles the [`ItchClient`] API calls
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
  /// The reqwest [`Response`]
  ///
  /// # Errors
  ///
  /// If the request fails to send
  pub(crate) fn itch_request(
    &self,
    url: &ItchApiUrl,
    method: Method,
    options: impl FnOnce(RequestBuilder) -> RequestBuilder,
  ) -> Result<Response, reqwest::Error> {
    // Create the base request
    let mut request: RequestBuilder = self.client.request(method, url.as_str());

    // Add authentication based on the API's version.
    request = match url.get_version() {
      // https://itchapi.ryhn.link/API/V1/index.html#authentication
      ItchApiVersion::V1 => {
        request.header(header::AUTHORIZATION, format!("Bearer {}", &self.api_key))
      }
      // https://itchapi.ryhn.link/API/V2/index.html#authentication
      ItchApiVersion::V2 => request.header(header::AUTHORIZATION, &self.api_key),
      // If it isn't a known API version, just leave it without authentication
      // Giving any authentication to an untrusted site is insecure because the API key could be stolen
      ItchApiVersion::Other => request,
    };

    // This header is set to ensure the use of the v2 version
    // https://itchapi.ryhn.link/API/V2/index.html
    if url.get_version() == ItchApiVersion::V2 {
      request = request.header(header::ACCEPT, "application/vnd.itch.v2");
    }

    // The callback is the final option before sending because
    // it needs to be able to modify anything
    request = options(request);

    request.send()
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
  fn itch_request_json<T>(
    &self,
    url: &ItchApiUrl,
    method: Method,
    options: impl FnOnce(RequestBuilder) -> RequestBuilder,
  ) -> Result<T, ItchRequestJSONError<<T as IntoResponseResult>::Err>>
  where
    T: serde::de::DeserializeOwned + IntoResponseResult,
  {
    // Get the response text
    let text = self
      .itch_request(url, method, options)
      .map_err(|e| ItchRequestJSONError {
        url: url.to_string(),
        kind: ItchRequestJSONErrorKind::CouldntSend(e),
      })?
      .text()
      .map_err(|e| ItchRequestJSONError {
        url: url.to_string(),
        kind: ItchRequestJSONErrorKind::CouldntGetText(e),
      })?;

    // Parse the response into JSON
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

/// This block defines the [`ItchClient`] constructors and other functions
impl ItchClient {
  /// Obtain the API key associated with this [`ItchClient`]
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
  /// An [`ItchClient`] struct with the given key
  #[must_use]
  pub fn new(api_key: String) -> Self {
    // Install the ring crypto provider
    // The function call fails if the provider has already been installed.
    // Ignore the error, because this function may be called more than once.
    let _ = rustls::crypto::ring::default_provider().install_default();

    Self {
      client: Client::new(),
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
  /// An [`ItchClient`] struct with the given key
  ///
  /// # Errors
  ///
  /// If the request, retrieving its text, or parsing fails, or if the server returned an error
  pub fn auth(api_key: String) -> Result<Self, ItchRequestJSONError<ApiResponseCommonErrors>> {
    let client = Self::new(api_key);

    // Verify that the API key is valid
    // Calling get_profile will fail if the given API key is invalid
    get_profile(&client)?;

    Ok(client)
  }
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
/// A [`LoginResponse`] enum with the response from the API, which can be either the API key or an error
///
/// # Errors
///
/// If the requests fail
pub fn login(
  client: &ItchClient,
  username: &str,
  password: &str,
  recaptcha_response: Option<&str>,
) -> Result<LoginResponse, ItchRequestJSONError<LoginResponseError>> {
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

  client.itch_request_json::<LoginResponse>(
    &ItchApiUrl::from_api_endpoint(ItchApiVersion::V2, "login"),
    Method::POST,
    |b| b.form(&params),
  )
}

/// Complete the login with the TOTP two-factor verification
///
/// # Arguments
///
/// * `totp_token` - The TOTP token returned by the previous login step
///
/// * `totp_code` - The 6-digit code returned by the TOTP application
///
/// # Returns
///
/// A [`LoginSuccess`] struct with the new API key
///
/// # Errors
///
/// If something goes wrong
pub fn totp_verification(
  client: &ItchClient,
  totp_token: &str,
  totp_code: u64,
) -> Result<LoginSuccess, ItchRequestJSONError<TOTPResponseError>> {
  client
    .itch_request_json::<TOTPResponse>(
      &ItchApiUrl::from_api_endpoint(ItchApiVersion::V2, "totp/verify"),
      Method::POST,
      |b| b.form(&[("token", totp_token), ("code", &totp_code.to_string())]),
    )
    .map(|res| res.success)
}

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
      &ItchApiUrl::from_api_endpoint(ItchApiVersion::V2, format!("users/{user_id}")),
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
    .itch_request_json::<ProfileInfoResponse>(
      &ItchApiUrl::from_api_endpoint(ItchApiVersion::V2, "profile"),
      Method::GET,
      |b| b,
    )
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
    .itch_request_json::<CreatedGamesResponse>(
      &ItchApiUrl::from_api_endpoint(ItchApiVersion::V2, "profile/games"),
      Method::GET,
      |b| b,
    )
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
      &ItchApiUrl::from_api_endpoint(ItchApiVersion::V2, "profile/owned-keys"),
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
      &ItchApiUrl::from_api_endpoint(ItchApiVersion::V2, "profile/collections"),
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
      &ItchApiUrl::from_api_endpoint(ItchApiVersion::V2, format!("collections/{collection_id}")),
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
      &ItchApiUrl::from_api_endpoint(
        ItchApiVersion::V2,
        format!("collections/{collection_id}/collection-games"),
      ),
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
      &ItchApiUrl::from_api_endpoint(ItchApiVersion::V2, format!("games/{game_id}")),
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
      &ItchApiUrl::from_api_endpoint(ItchApiVersion::V2, format!("games/{game_id}/uploads")),
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
      &ItchApiUrl::from_api_endpoint(ItchApiVersion::V2, format!("uploads/{upload_id}")),
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
      &ItchApiUrl::from_api_endpoint(ItchApiVersion::V2, format!("uploads/{upload_id}/builds")),
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
      &ItchApiUrl::from_api_endpoint(ItchApiVersion::V2, format!("builds/{build_id}")),
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
      &ItchApiUrl::from_api_endpoint(
        ItchApiVersion::V2,
        format!("builds/{current_build_id}/upgrade-paths/{target_build_id}"),
      ),
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
      &ItchApiUrl::from_api_endpoint(
        ItchApiVersion::V2,
        format!("uploads/{upload_id}/scanned-archive"),
      ),
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
      &ItchApiUrl::from_api_endpoint(
        ItchApiVersion::V2,
        format!("builds/{build_id}/scanned-archive"),
      ),
      Method::GET,
      |b| b,
    )
    .map(|res| res.scanned_archive)
}
