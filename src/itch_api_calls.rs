use crate::itch_api_types::*;
use reqwest::{Method, Response, header};

/// A client able to send requests to the itch.io API
pub struct ItchClient {
  client: reqwest::Client,
  api_key: String,
}

// This block defines the ItchClient API calls and helper functions
impl ItchClient {
  /// Obtain the API key associated with this `ItchClient`
  pub fn get_api_key(&self) -> &str {
    &self.api_key
  }

  /// Make a request to the itch.io API
  ///
  /// # Arguments
  ///
  /// * `url` - A itch.io API address to make the request against
  ///
  /// * `method` - The request method (GET, POST, etc.)
  ///
  /// * `options` - A closure that modifies the request builder just before sending it
  ///
  /// # Returns
  ///
  /// The reqwest `Response`
  ///
  /// An error if sending the request fails
  pub(crate) async fn itch_request(
    &self,
    url: &ItchApiUrl<'_>,
    method: Method,
    options: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
  ) -> Result<Response, String> {
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

    request
      .send()
      .await
      .map_err(|e| format!("Error while sending request: {e}"))
  }

  /// Make a request to the itch.io API and parse the response as JSON
  ///
  /// # Arguments
  ///
  /// * `url` - A itch.io API address to make the request against
  ///
  /// * `method` - The request method (GET, POST, etc.)
  ///
  /// * `options` - A closure that modifies the request builder just before sending it
  ///
  /// # Returns
  ///
  /// The JSON response parsed into the provided type
  ///
  /// An error if sending the request or parsing it fails
  async fn itch_request_json<T>(
    &self,
    url: &ItchApiUrl<'_>,
    method: Method,
    options: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
  ) -> Result<T, String>
  where
    T: serde::de::DeserializeOwned,
  {
    let text = self
      .itch_request(url, method, options)
      .await?
      .text()
      .await
      .map_err(|e| format!("Error while reading response body: {e}"))?;

    serde_json::from_str::<ApiResponse<T>>(&text)
      .map_err(|e| format!("Error while parsing JSON body: {e}\n\n{}", text))?
      .into_result()
  }
}

// This block defines the ItchClient constructors
impl ItchClient {
  /// Create a new client with the given itch.io API key, and verifies that it is valid
  ///
  /// # Arguments
  ///
  /// * `api_key` - A valid itch.io API key to store in the client
  ///
  /// # Returns
  ///
  /// A `ItchClient` struct with the given key
  ///
  /// An error if the API key is invalid or couldn't be verified
  pub async fn auth(api_key: String) -> Result<Self, String> {
    let client = Self {
      client: reqwest::Client::new(),
      api_key,
    };

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
  ) -> Result<LoginSuccess, String> {
    self
      .itch_request_json::<LoginSuccess>(&ItchApiUrl::V2("totp/verify"), Method::POST, |b| {
        b.form(&[("token", totp_token), ("code", &totp_code.to_string())])
      })
      .await
      .map_err(|e| format!("An error occurred while attempting log in:\n{e}"))
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
  /// * `recaptcha_response` - If required, the reCAPTCHA token from https://itch.io/captcha
  ///
  /// * `totp_code` - If required, The 6-digit code returned by the TOTP application
  ///
  /// # Returns
  ///
  /// An `ItchClient` struct with the new API key
  ///
  /// An error if something goes wrong
  pub async fn login(
    username: &str,
    password: &str,
    recaptcha_response: Option<&str>,
    totp_code: Option<u64>,
  ) -> Result<Self, String> {
    let mut client = Self {
      client: reqwest::Client::new(),
      api_key: String::new(),
    };

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

        client
          .totp_verification(e.token.as_str(), totp_code)
          .await?
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
/// An error if something goes wrong
pub async fn get_profile(client: &ItchClient) -> Result<User, String> {
  client
    .itch_request_json::<ProfileResponse>(&ItchApiUrl::V2("profile"), Method::GET, |b| b)
    .await
    .map(|res| res.user)
    .map_err(|e| format!("An error occurred while attempting to get the profile info:\n{e}"))
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
/// An error if something goes wrong
pub async fn get_owned_keys(client: &ItchClient) -> Result<Vec<OwnedKey>, String> {
  let mut keys: Vec<OwnedKey> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let mut response = client
      .itch_request_json::<OwnedKeysResponse>(
        &ItchApiUrl::V2("profile/owned-keys"),
        Method::GET,
        |b| b.query(&[("page", page)]),
      )
      .await
      .map_err(|e| {
        format!(
          "An error occurred while attempting to obtain the list of the user's game keys:\n{e}"
        )
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
/// * `client` - An itch.io API client
///
/// # Returns
///
/// A vector of `CreatedGame` structs with the info provided by the API
///
/// An error if something goes wrong
pub async fn get_crated_games(client: &ItchClient) -> Result<Vec<CreatedGame>, String> {
  client
    .itch_request_json::<CreatedGamesResponse>(&ItchApiUrl::V2("profile/games"), Method::GET, |b| b)
    .await
    .map(|res| res.games)
    .map_err(|e| {
      format!(
        "An error occurred while attempting to obtain the list of the user created games:\n{e}"
      )
    })
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
/// An error if something goes wrong
pub async fn get_game_info(client: &ItchClient, game_id: u64) -> Result<Game, String> {
  client
    .itch_request_json::<GameInfoResponse>(
      &ItchApiUrl::V2(&format!("games/{game_id}")),
      Method::GET,
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
/// * `client` - An itch.io API client
///
/// * `game_id` - The ID of the game from which information will be obtained
///
/// # Returns
///
/// A vector of `Upload` structs with the info provided by the API
///
/// An error if something goes wrong
pub async fn get_game_uploads(client: &ItchClient, game_id: u64) -> Result<Vec<Upload>, String> {
  client
    .itch_request_json::<GameUploadsResponse>(
      &ItchApiUrl::V2(&format!("games/{game_id}/uploads")),
      Method::GET,
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
/// * `client` - An itch.io API client
///
/// * `upload_id` - The ID of the upload from which information will be obtained
///
/// # Returns
///
/// A `Upload` struct with the info provided by the API
///
/// An error if something goes wrong
pub async fn get_upload_info(client: &ItchClient, upload_id: u64) -> Result<Upload, String> {
  client
    .itch_request_json::<UploadResponse>(
      &ItchApiUrl::V2(&format!("uploads/{upload_id}")),
      Method::GET,
      |b| b,
    )
    .await
    .map(|res| res.upload)
    .map_err(|e| {
      format!("An error occurred while attempting to obtain the upload information:\n{e}")
    })
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
/// An error if something goes wrong
pub async fn get_collections(client: &ItchClient) -> Result<Vec<Collection>, String> {
  client
    .itch_request_json::<CollectionsResponse>(
      &ItchApiUrl::V2("profile/collections"),
      Method::GET,
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
/// * `client` - An itch.io API client
///
/// * `collection_id` - The ID of the collection from which information will be obtained
///
/// # Returns
///
/// A vector of `CollectionGameItem` structs with the info provided by the API
///
/// An error if something goes wrong
pub async fn get_collection_games(
  client: &ItchClient,
  collection_id: u64,
) -> Result<Vec<CollectionGameItem>, String> {
  let mut games: Vec<CollectionGameItem> = Vec::new();
  let mut page: u64 = 1;
  loop {
    let mut response = client
      .itch_request_json::<CollectionGamesResponse>(
        &ItchApiUrl::V2(&format!("collections/{collection_id}/collection-games")),
        Method::GET,
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
