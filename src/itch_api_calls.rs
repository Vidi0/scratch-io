use crate::itch_api_types::*;
use reqwest::{Client, Method, Response, header};

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
  options: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
) -> Result<Response, String> {
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

  request
    .send()
    .await
    .map_err(|e| format!("Error while sending request: {e}"))
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
/// An error if sending the request or parsing it fails
async fn itch_request_json<T>(
  client: &Client,
  method: Method,
  url: &ItchApiUrl<'_>,
  api_key: &str,
  options: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
) -> Result<T, String>
where
  T: serde::de::DeserializeOwned,
{
  let text = itch_request(client, method, url, api_key, options)
    .await?
    .text()
    .await
    .map_err(|e| format!("Error while reading response body: {e}"))?;

  serde_json::from_str::<ApiResponse<T>>(&text)
    .map_err(|e| format!("Error while parsing JSON body: {e}\n\n{}", text))?
    .into_result()
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
async fn totp_verification(
  client: &Client,
  totp_token: &str,
  totp_code: u64,
) -> Result<LoginSuccess, String> {
  itch_request_json::<LoginSuccess>(
    client,
    Method::POST,
    &ItchApiUrl::V2("totp/verify"),
    "",
    |b| b.form(&[("token", totp_token), ("code", &totp_code.to_string())]),
  )
  .await
  .map_err(|e| format!("An error occurred while attempting log in:\n{e}"))
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
pub async fn login(
  client: &Client,
  username: &str,
  password: &str,
  recaptcha_response: Option<&str>,
  totp_code: Option<u64>,
) -> Result<LoginSuccess, String> {
  let mut params: Vec<(&'static str, &str)> = vec![
    ("username", username),
    ("password", password),
    ("force_recaptcha", "false"),
    ("source", "desktop"),
  ];

  if let Some(rr) = recaptcha_response {
    params.push(("recaptcha_response", rr));
  }

  let response =
    itch_request_json::<LoginResponse>(client, Method::POST, &ItchApiUrl::V2("login"), "", |b| {
      b.form(&params)
    })
    .await
    .map_err(|e| format!("An error occurred while attempting log in:\n{e}"))?;

  let ls = match response {
    LoginResponse::CaptchaError(e) => {
      return Err(format!(
        r#"A reCAPTCHA verification is required to continue!
  Go to "{}" and solve the reCAPTCHA.
  To obtain the token, paste the following command on the developer console:
    console.log(grecaptcha.getResponse())
  Then run the login command again with the --recaptcha-response option."#,
        e.recaptcha_url.as_str()
      ));
    }
    LoginResponse::TOTPError(e) => {
      let Some(totp_code) = totp_code else {
        return Err(
          r#"The accout has 2 step verification enabled via TOTP
  Run the login command again with the --totp-code={{VERIFICATION_CODE}} option."#
            .to_string(),
        );
      };

      totp_verification(client, e.token.as_str(), totp_code).await?
    }
    LoginResponse::Success(ls) => ls,
  };

  Ok(ls)
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
pub async fn get_profile(client: &Client, api_key: &str) -> Result<User, String> {
  itch_request_json::<ProfileResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2("profile"),
    api_key,
    |b| b,
  )
  .await
  .map(|res| res.user)
  .map_err(|e| format!("An error occurred while attempting to get the profile info:\n{e}"))
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
pub async fn get_owned_keys(client: &Client, api_key: &str) -> Result<Vec<OwnedKey>, String> {
  let mut keys: Vec<OwnedKey> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let mut response = itch_request_json::<OwnedKeysResponse>(
      client,
      Method::GET,
      &ItchApiUrl::V2("profile/owned-keys"),
      api_key,
      |b| b.query(&[("page", page)]),
    )
    .await
    .map_err(|e| {
      format!("An error occurred while attempting to obtain the list of the user's game keys:\n{e}")
    })?;

    let num_keys: u64 = response.owned_keys.len() as u64;
    keys.append(&mut response.owned_keys);
    // Warning!!!
    // response.collection_games was merged into games, but it WAS NOT dropped!
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
/// A vector of CreatedGame structs with the info provided by the API
///
/// An error if something goes wrong
pub async fn get_crated_games(client: &Client, api_key: &str) -> Result<Vec<CreatedGame>, String> {
  itch_request_json::<CreatedGamesResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2("profile/games"),
    api_key,
    |b| b,
  )
  .await
  .map(|res| res.games)
  .map_err(|e| {
    format!("An error occurred while attempting to obtain the list of the user created games:\n{e}")
  })
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
pub async fn get_game_info(client: &Client, api_key: &str, game_id: u64) -> Result<Game, String> {
  itch_request_json::<GameInfoResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2(&format!("games/{game_id}")),
    api_key,
    |b| b,
  )
  .await
  .map(|res| res.game)
  .map_err(|e| format!("An error occurred while attempting to obtain the game info:\n{e}"))
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
pub async fn get_game_uploads(
  client: &Client,
  api_key: &str,
  game_id: u64,
) -> Result<Vec<Upload>, String> {
  itch_request_json::<GameUploadsResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2(&format!("games/{game_id}/uploads")),
    api_key,
    |b| b,
  )
  .await
  .map(|res| res.uploads)
  .map_err(|e| format!("An error occurred while attempting to obtain the game uploads:\n{e}"))
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
pub async fn get_upload_info(
  client: &Client,
  api_key: &str,
  upload_id: u64,
) -> Result<Upload, String> {
  itch_request_json::<UploadResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2(&format!("uploads/{upload_id}")),
    api_key,
    |b| b,
  )
  .await
  .map(|res| res.upload)
  .map_err(|e| format!("An error occurred while attempting to obtain the upload information:\n{e}"))
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
pub async fn get_collections(client: &Client, api_key: &str) -> Result<Vec<Collection>, String> {
  itch_request_json::<CollectionsResponse>(
    client,
    Method::GET,
    &ItchApiUrl::V2("profile/collections"),
    api_key,
    |b| b,
  )
  .await
  .map(|res| res.collections)
  .map_err(|e| {
    format!(
      "An error occurred while attempting to obtain the list of the profile's collections:\n{e}"
    )
  })
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
pub async fn get_collection_games(
  client: &Client,
  api_key: &str,
  collection_id: u64,
) -> Result<Vec<CollectionGameItem>, String> {
  let mut games: Vec<CollectionGameItem> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let mut response = itch_request_json::<CollectionGamesResponse>(
      client,
      Method::GET,
      &ItchApiUrl::V2(&format!("collections/{collection_id}/collection-games")),
      api_key,
      |b| b.query(&[("page", page)]),
    )
    .await
    .map_err(|e| {
      format!(
        "An error occurred while attempting to obtain the list of the collection's games: {e}"
      )
    })?;

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
