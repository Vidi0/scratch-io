pub mod errors;
mod responses;
pub mod types;

use errors::*;
use responses::*;
pub use responses::{ApiResponse, IntoResponseResult};
use types::*;

use reqwest::{Method, Response, header};

/// A client able to send requests to the itch.io API
pub struct ItchClient {
  client: reqwest::Client,
  api_key: String,
}

/// This block defiles the ItchClient API calls
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
  /// If sending the request fails
  pub(crate) async fn itch_request(
    &self,
    url: &ItchApiUrl<'_>,
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
  /// If sending the request or parsing it fails
  async fn itch_request_json<T>(
    &self,
    url: &ItchApiUrl<'_>,
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

  /// Make requests to the itch.io API to get a list of items that are split into pages
  ///
  /// Takes any type that implements `ListResponse`
  ///
  /// # Arguments
  ///
  /// * `url` - An itch.io API address to make the requests against
  ///
  /// * `method` - The request method (GET, POST, etc.)
  ///
  /// # Returns
  ///
  /// A Vector of the corresponding `ListResponse::Item` structs
  ///
  /// # Errors
  ///
  /// If sending the request or parsing it fails
  async fn itch_request_list<T>(
    &self,
    url: &ItchApiUrl<'_>,
    method: Method,
    mut options: impl FnMut(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
  ) -> Result<Vec<T::Item>, ItchRequestJSONError<<ApiResponseList<T> as IntoResponseResult>::Err>>
  where
    T: serde::de::DeserializeOwned + ListResponse,
    ApiResponseList<T>: IntoResponseResult,
  {
    let mut values: Vec<T::Item> = Vec::new();
    let mut page: u64 = 1;
    loop {
      let response = self
        .itch_request_json::<ApiResponseList<T>>(url, method.clone(), |b| {
          options(b.query(&[("page", page)]))
        })
        .await?;

      let response_values = response.values.items();
      let num_elements: u64 = response_values.len() as u64;
      values.extend(response_values.into_iter());

      if num_elements < response.per_page || num_elements == 0 {
        break;
      }

      page += 1;
    }

    Ok(values)
  }
}

/// This block defines the ItchClient constructors and other functions
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
  /// If the API key is invalid or couldn't be verified
  pub async fn auth(
    api_key: String,
  ) -> Result<Self, ItchRequestJSONError<ApiResponseCommonErrors>> {
    let client = ItchClient::new(api_key);

    // Verify that the API key is valid
    // Calling get_profile will fail if the given API key is invalid
    get_profile(&client).await?;

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
      .itch_request_json::<TOTPResponse>(&ItchApiUrl::V2("totp/verify"), Method::POST, |b| {
        b.form(&[("token", totp_token), ("code", &totp_code.to_string())])
      })
      .await
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
  /// If something goes wrong
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
      ("source", "desktop"),
    ];

    if let Some(rr) = recaptcha_response {
      params.push(("recaptcha_response", rr));
    }

    let response = client
      .itch_request_json::<LoginResponse>(&ItchApiUrl::V2("login"), Method::POST, |b| {
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
          .totp_verification(e.token.as_str(), totp_code)
          .await
          .map(|res| res.success)?
      }
      LoginResponse::Success(ls) => ls,
    };

    // Save the new API key into the client for future API calls
    client.api_key = ls.key.key;

    Ok(client)
  }
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
/// A `User` struct with the info provided by the API
///
/// # Errors
///
/// If something goes wrong
pub async fn get_profile(
  client: &ItchClient,
) -> Result<User, ItchRequestJSONError<ApiResponseCommonErrors>> {
  client
    .itch_request_json::<ProfileInfoResponse>(&ItchApiUrl::V2("profile"), Method::GET, |b| b)
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
/// If something goes wrong
pub async fn get_created_games(
  client: &ItchClient,
) -> Result<Vec<CreatedGame>, ItchRequestJSONError<ApiResponseCommonErrors>> {
  client
    .itch_request_json::<CreatedGamesResponse>(&ItchApiUrl::V2("profile/games"), Method::GET, |b| b)
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
/// If something goes wrong
pub async fn get_owned_keys(
  client: &ItchClient,
) -> Result<Vec<OwnedKey>, ItchRequestJSONError<ApiResponseCommonErrors>> {
  client
    .itch_request_list::<OwnedKeysResponse>(
      &ItchApiUrl::V2("profile/owned-keys"),
      Method::GET,
      |b| b,
    )
    .await
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
/// If something goes wrong
pub async fn get_profile_collections(
  client: &ItchClient,
) -> Result<Vec<Collection>, ItchRequestJSONError<ApiResponseCommonErrors>> {
  client
    .itch_request_json::<ProfileCollectionsResponse>(
      &ItchApiUrl::V2("profile/collections"),
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
/// If something goes wrong
pub async fn get_collection_info(
  client: &ItchClient,
  collection_id: u64,
) -> Result<Collection, ItchRequestJSONError<CollectionResponseError>> {
  client
    .itch_request_json::<CollectionInfoResponse>(
      &ItchApiUrl::V2(&format!("collections/{collection_id}")),
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
/// If something goes wrong
pub async fn get_collection_games(
  client: &ItchClient,
  collection_id: u64,
) -> Result<Vec<CollectionGameItem>, ItchRequestJSONError<CollectionResponseError>> {
  client
    .itch_request_list::<CollectionGamesResponse>(
      &ItchApiUrl::V2(&format!("collections/{collection_id}/collection-games")),
      Method::GET,
      |b| b,
    )
    .await
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
/// If something goes wrong
pub async fn get_game_info(
  client: &ItchClient,
  game_id: u64,
) -> Result<Game, ItchRequestJSONError<GameResponseError>> {
  client
    .itch_request_json::<GameInfoResponse>(
      &ItchApiUrl::V2(&format!("games/{game_id}")),
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
/// If something goes wrong
pub async fn get_game_uploads(
  client: &ItchClient,
  game_id: u64,
) -> Result<Vec<Upload>, ItchRequestJSONError<GameResponseError>> {
  client
    .itch_request_json::<GameUploadsResponse>(
      &ItchApiUrl::V2(&format!("games/{game_id}/uploads")),
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
/// If something goes wrong
pub async fn get_upload_info(
  client: &ItchClient,
  upload_id: u64,
) -> Result<Upload, ItchRequestJSONError<UploadResponseError>> {
  client
    .itch_request_json::<UploadInfoResponse>(
      &ItchApiUrl::V2(&format!("uploads/{upload_id}")),
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
/// If something goes wrong
pub async fn get_upload_builds(
  client: &ItchClient,
  upload_id: u64,
) -> Result<Vec<UploadBuild>, ItchRequestJSONError<UploadResponseError>> {
  client
    .itch_request_json::<UploadBuildsResponse>(
      &ItchApiUrl::V2(&format!("uploads/{upload_id}/builds")),
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
/// If something goes wrong
pub async fn get_build_info(
  client: &ItchClient,
  build_id: u64,
) -> Result<Build, ItchRequestJSONError<BuildResponseError>> {
  client
    .itch_request_json::<BuildInfoResponse>(
      &ItchApiUrl::V2(&format!("builds/{build_id}")),
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
/// If something goes wrong
pub async fn get_upgrade_path(
  client: &ItchClient,
  current_build_id: u64,
  target_build_id: u64,
) -> Result<Vec<UpgradePathBuild>, ItchRequestJSONError<UpgradePathResponseError>> {
  client
    .itch_request_json::<BuildUpgradePathResponse>(
      &ItchApiUrl::V2(&format!(
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

  /// Read the `SCRATCH_API_KEY` environment variable and create a `ItchClient` based on it
  ///
  /// # Panics
  ///
  /// If the `SCRATCH_API_KEY` environment variable isn't set
  async fn get_client() -> ItchClient {
    let api_key = std::env::var("SCRATCH_API_KEY").expect("SCRATCH_API_KEY must be set for tests");
    ItchClient::new(api_key)
  }

  #[tokio::test]
  async fn test_profile() {
    let client = get_client().await;
    get_profile(&client).await.unwrap();
  }
}
