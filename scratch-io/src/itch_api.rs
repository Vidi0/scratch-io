pub mod endpoints;
pub mod errors;
pub mod oauth;
pub mod types;

mod responses;

use errors::{ItchRequestJSONError, ItchRequestJSONErrorKind};
use responses::{ApiResponse, IntoResponseResult};

use reqwest::{
  Method,
  blocking::{Client, RequestBuilder, Response},
  header,
};

pub const ITCH_API_V1_BASE_URL: &str = "https://itch.io/api/1/";
pub const ITCH_API_V2_BASE_URL: &str = "https://api.itch.io/";

/// An itch.io API version
///
/// Its possible values are:
///
/// * `V1` - itch.io JSON API V1 <https://itch.io/api/1/>
///
/// * `V2` - itch.io JSON API V2 <https://api.itch.io/>
///
/// * `Other` - Any other URL
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItchApiVersion {
  V1,
  V2,
  Other,
}

/// An itch.io API address
///
/// Use the Other variant with the full URL when it isn't a known API version
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItchApiUrl {
  version: ItchApiVersion,
  url: String,
}

impl ItchApiUrl {
  /// Creates an [`ItchApiUrl`] by appending the provided endpoint to
  /// itch.io's API V1 base URL: [`ITCH_API_V1_BASE_URL`]
  ///
  /// <https://itch.io/api/1/>
  pub fn v1(endpoint: &str) -> Self {
    Self {
      version: ItchApiVersion::V1,
      url: format!("{ITCH_API_V1_BASE_URL}{endpoint}"),
    }
  }

  /// Creates an [`ItchApiUrl`] by appending the provided endpoint to
  /// itch.io's API V2 base URL: [`ITCH_API_V2_BASE_URL`]
  ///
  /// <https://api.itch.io/>
  pub fn v2(endpoint: &str) -> Self {
    Self {
      version: ItchApiVersion::V2,
      url: format!("{ITCH_API_V2_BASE_URL}{endpoint}"),
    }
  }

  /// Creates an [`ItchApiUrl`] using the provided url as-is
  pub fn other(url: String) -> Self {
    Self {
      version: ItchApiVersion::Other,
      url,
    }
  }

  /// Returns the API version of this [`ItchApiUrl`]
  #[must_use]
  pub const fn version(&self) -> ItchApiVersion {
    self.version
  }
}

impl ItchApiUrl {
  /// Get a reference to the full URL string
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.url
  }
}

impl std::fmt::Display for ItchApiUrl {
  /// Format the [`ItchApiUrl`] as a string, returning the full URL
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.url)
  }
}

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
    request = match url.version() {
      // https://itchapi.ryhn.link/API/V1/index.html#authentication
      ItchApiVersion::V1 => request.bearer_auth(&self.api_key),
      // https://itchapi.ryhn.link/API/V2/index.html#authentication
      ItchApiVersion::V2 => request.header(header::AUTHORIZATION, &self.api_key),
      // If it isn't a known API version, just leave it without authentication
      // Giving any authentication to an untrusted site is insecure because the API key could be stolen
      ItchApiVersion::Other => request,
    };

    // This header is set to ensure the use of the v2 version
    // https://itchapi.ryhn.link/API/V2/index.html
    if url.version() == ItchApiVersion::V2 {
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
  /// Create a new unauthenticated client
  ///
  /// The client will not be able to make API calls that require an API key
  ///
  /// # Returns
  ///
  /// An [`ItchClient`] struct with an empty API key
  pub fn unauthenticated() -> Self {
    Self {
      client: Client::new(),
      api_key: String::new(),
    }
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
    Self {
      api_key,
      ..Self::unauthenticated()
    }
  }

  /// Obtain the API key associated with this [`ItchClient`]
  #[must_use]
  pub fn api_key(&self) -> &str {
    &self.api_key
  }
}
