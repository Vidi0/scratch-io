use crate::itch_api_types::*;
use reqwest::{Client, Method, Response, header};
use thiserror::Error;

#[derive(Error, Debug)]
#[error("Error while sending request, redirect loop was detected or redirect limit was exhausted:\n{url}\n{error}")]
pub struct ItchRequestError {
  url: String,
  #[source]
  error: reqwest::Error,
}

/// Make a request to the itch.io API
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `method` - The request method (GET, POST, etc.)
/// 
/// * `url` - A itch.io API address to make the request against
/// 
/// * `api_key` - A valid itch.io API key to make the request
/// 
/// * `options` - A closure that modifies the request builder just before sending it
/// 
/// # Returns
/// 
/// The reqwest response
/// 
/// An error if sending the request fails
pub(crate) async fn itch_request(
  client: &Client,
  method: Method,
  url: &ItchApiUrl<'_>,
  api_key: &str,
  options: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder
) -> Result<Response, ItchRequestError> {
  let mut request: reqwest::RequestBuilder = client.request(method, url.to_string());

  // Add authentication based on the API's version.
  request = match url {
    // https://itchapi.ryhn.link/API/V1/index.html#authentication
    ItchApiUrl::V1(..) => request.header(header::AUTHORIZATION, format!("Bearer {api_key}")),
    // https://itchapi.ryhn.link/API/V2/index.html#authentication
    ItchApiUrl::V2(..) => request.header(header::AUTHORIZATION, api_key),
    // If it isn't a known API version, just leave it without authentication
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
    .map_err(|error| ItchRequestError { url: url.to_string(), error })
}

#[derive(Error, Debug)]
pub enum ItchRequestJSONError<T> where
  ApiResponse<T>: IntoApiResult,
{
  #[error(transparent)]
  CouldntSend(#[from] ItchRequestError),

  #[error("Couldn't get the network request response body:\n{url}\n{error}")]
  CouldntGetText {
    url: String,
    #[source]
    error: reqwest::Error,
  },

  #[error("Couldn't parse the request response body into JSON:\n{body}\n\n{error}")]
  InvalidJSON {
    body: String,
    #[source]
    error: serde_json::Error,
  },

  #[error("The itch.io API server returned an error:\n{0}")]
  ServerRepliedWithError(<ApiResponse<T> as IntoApiResult>::Err)
}

/// Make a request to the itch.io API and parse the response as json
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `method` - The request method (GET, POST, etc.)
/// 
/// * `url` - A itch.io API address to make the request against
/// 
/// * `api_key` - A valid itch.io API key to make the request
/// 
/// * `options` - A closure that modifies the request builder just before sending it
/// 
/// # Returns
/// 
/// The reqwest response parsed as JSON into the provided type
/// 
/// An error if sending the request or parsing it fails, or if the server returns an error
async fn itch_request_json<T>(
  client: &Client,
  method: Method,
  url: &ItchApiUrl<'_>,
  api_key: &str,
  options: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
) -> Result<<ApiResponse<T> as IntoApiResult>::Ok, ItchRequestJSONError<T>> where
  // T must be deserializable
  T: serde::de::DeserializeOwned,
  // The ApiResponse for T must be parsed into a Result
  ApiResponse<T>: IntoApiResult,
{
  let text = itch_request(client, method, url, api_key, options).await?
    .text().await
    .map_err(|error| ItchRequestJSONError::CouldntGetText { url: url.to_string(), error })?;

  serde_json::from_str::<ApiResponse<T>>(&text)
    .map_err(|error| ItchRequestJSONError::InvalidJSON { body: text, error })?
    .into_result()
    .map_err(|e| ItchRequestJSONError::ServerRepliedWithError(e))
}

/// Complete the login with the TOTP 2nd factor verification
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `totp_token` - The TOTP token returned by the previous login step
/// 
/// * `totp_code` - The 6-digit code returned by the TOTP application
/// 
/// # Returns
/// 
/// A LoginSuccess struct with the new API key
/// 
/// An error if something goes wrong
async fn totp_verification(client: &Client, totp_token: &str, totp_code: u64) -> Result<LoginSuccess, ItchRequestJSONError<LoginResponse>> {
  itch_request_json::<LoginResponse>(
    client,
    Method::POST,
    &ItchApiUrl::V2("totp/verify"),
    "",
    |b| b.form(&[
      ("token", totp_token),
      ("code", &totp_code.to_string())
    ]),
  ).await
}

/// Login to itch.io
/// 
/// Retrieve a API key from a username and password authentication
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `username` - The username OR email of the accout to log in with
/// 
/// * `password` - The password of the accout to log in with
/// 
/// * `recaptcha_response` - If required, the reCAPTCHA token from https://itch.io/captcha
/// 
/// * `totp_code` - If required, The 6-digit code returned by the TOTP application
/// 
/// # Returns
/// 
/// A LoginSuccess struct with the new API key
/// 
/// An error if something goes wrong
pub async fn login(client: &Client, username: &str, password: &str, recaptcha_response: Option<&str>, totp_code: Option<u64>) -> Result<LoginSuccess, ItchRequestJSONError<LoginResponse>> {
  let mut params: Vec<(&'static str, &str)> = vec![
    ("username", username),
    ("password", password),
    ("force_recaptcha", "false"),
    ("source", "desktop"),
  ];

  if let Some(rr) = recaptcha_response {
    params.push(("recaptcha_response", rr));
  }

  let response = itch_request_json::<LoginResponse>(
    client,
    Method::POST,
    &ItchApiUrl::V2("login"),
    "",
    |b| b.form(&params),
  ).await;

  match (response, totp_code) {
    (Ok(ls), _) => Ok(ls),
    (
      Err(ItchRequestJSONError::ServerRepliedWithError(
        LoginResponseError::TOTPNeeded(lte),
      )),
      Some(totp_code),
    ) => totp_verification(client, &lte.token, totp_code).await,
    (Err(e), _) => Err(e),
  }
}

/// Get the API key's profile
/// 
/// This can be used to verify that a given itch.io API key is valid
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid itch.io API key to make the request
/// 
/// # Returns
/// 
/// A User struct with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_profile(client: &Client, api_key: &str) -> Result<User, ItchRequestJSONError<ProfileResponse>> {
  itch_request_json::<ProfileResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2("profile"),
    api_key,
    |b| b,
  ).await
    .map(|res| res.user)
}

/// Get the user's owned game keys
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid itch.io API key to make the request
/// 
/// # Returns
/// 
/// A vector of OwnedKey structs with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_owned_keys(client: &Client, api_key: &str) -> Result<Vec<OwnedKey>, ItchRequestJSONError<OwnedKeysResponse>> {
  let mut keys: Vec<OwnedKey> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let mut response = itch_request_json::<OwnedKeysResponse>(
      client,
      Method::GET,
      &ItchApiUrl::V2("profile/owned-keys"),
      api_key,
      |b| b.query(&[("page", page)]),
    ).await?;

    let num_keys: u64 = response.owned_keys.len() as u64;
    keys.append(&mut response.owned_keys);
    // Warning!!!
    // response.owned_keys was merged into games, but it WAS NOT dropped!
    // Its length is still accessible, but this doesn't make sense!
    
    if num_keys < response.per_page || num_keys == 0 {
      break;
    }
    page += 1;
  }

  Ok(keys)
}

/// Get the games that the user created or that the user is an admin of
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid itch.io API key to make the request
/// 
/// # Returns
/// 
/// A vector of OwnedKey structs with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_crated_games(client: &Client, api_key: &str) -> Result<Vec<CreatedGame>, ItchRequestJSONError<CreatedGamesResponse>> {
  itch_request_json::<CreatedGamesResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2("profile/games"),
    api_key,
    |b| b,
  ).await
    .map(|res| res.games)
}

/// List the user's game collections
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid itch.io API key to make the request
/// 
/// # Returns
/// 
/// A vector of Collection structs with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_collections(client: &Client, api_key: &str) -> Result<Vec<Collection>, ItchRequestJSONError<CollectionsResponse>> {
  itch_request_json::<CollectionsResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2("profile/collections"),
    api_key,
    |b| b,
  ).await
    .map(|res| res.collections)
}

/// Get the information about a game in itch.io
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid itch.io API key to make the request
/// 
/// * `game_id` - The ID of the game from which information will be obtained
/// 
/// # Returns
/// 
/// A Game struct with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_game_info(client: &Client, api_key: &str, game_id: u64) -> Result<Game, ItchRequestJSONError<GameInfoResponse>> {
  itch_request_json::<GameInfoResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2(&format!("games/{game_id}")),
    api_key,
    |b| b,
  ).await
    .map(|res| res.game)
}

/// Get the game's uploads (downloadable files)
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid itch.io API key to make the request
/// 
/// * `game_id` - The ID of the game from which information will be obtained
/// 
/// # Returns
/// 
/// A vector of Upload structs with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_game_uploads(client: &Client, api_key: &str, game_id: u64) -> Result<Vec<Upload>, ItchRequestJSONError<GameUploadsResponse>> {
  itch_request_json::<GameUploadsResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2(&format!("games/{game_id}/uploads")),
    api_key,
    |b| b,
  ).await
    .map(|res| res.uploads)
}

/// Get an upload's info
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid itch.io API key to make the request
/// 
/// * `upload_id` - The ID of the upload from which information will be obtained
/// 
/// # Returns
/// 
/// A Upload struct with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_upload_info(client: &Client, api_key: &str, upload_id: u64) -> Result<Upload, ItchRequestJSONError<UploadResponse>> {
  itch_request_json::<UploadResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2(&format!("uploads/{upload_id}")),
    api_key,
    |b| b,
  ).await
    .map(|res| res.upload)
}

/// List a collection's games
/// 
/// # Arguments
/// 
/// * `client` - An asynchronous reqwest Client
/// 
/// * `api_key` - A valid itch.io API key to make the request
/// 
/// * `collection_id` - The ID of the collection from which information will be obtained
/// 
/// # Returns
/// 
/// A vector of CollectionGameItem structs with the info provided by the API
/// 
/// An error if something goes wrong
pub async fn get_collection_games(client: &Client, api_key: &str, collection_id: u64) -> Result<Vec<CollectionGameItem>, ItchRequestJSONError<CollectionGamesResponse>> {   
  let mut games: Vec<CollectionGameItem> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let mut response = itch_request_json::<CollectionGamesResponse>(
      client,
      Method::GET,
      &ItchApiUrl::V2(&format!("collections/{collection_id}/collection-games")),
      api_key,
      |b| b.query(&[("page", page)]),
    ).await?;

    let num_games: u64 = response.collection_games.len() as u64;
    games.append(&mut response.collection_games);
    // Warning!!!
    // response.collection_games was merged into games, but it WAS NOT dropped!
    // Its length is still accessible, but this doesn't make sense!
    
    if num_games < response.per_page || num_games == 0 {
      break;
    }
    page += 1;
  }

  Ok(games)
}

/// Get the url to a itch.io web game
/// 
/// # Arguments
/// 
/// * `upload_id` - The ID of the html upload
/// 
/// # Returns
/// 
/// The web game URL
pub fn get_web_game_url(upload_id: u64) -> String {
  format!("https://html-classic.itch.zone/html/{upload_id}/index.html")
}
