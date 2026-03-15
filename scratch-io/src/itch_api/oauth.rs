mod code_verifier;

pub use code_verifier::{CodeChallenge, CodeVerifier};

use reqwest::Url;

/// OAuth client ID used by the itch.io APP
///
/// <https://github.com/itchio/itch/blob/3a9c33a654e55e039bc0ae5155d83fb0ddd1aca2/src/main/reactors/login.ts#L29>
const CLIENT_ID: &str = "85252daf268d27fbefac93e1ac462bfd";
const RESPONSE_TYPE: &str = "code";
const SCOPE: &str = "itch";
const REDIRECT_URI: &str = "itch://oauth-callback";
/// itch.io OAuth 2.0 authorization endpoint
const URL_ENDPOINT: &str = "https://itch.io/user/oauth";
/// SHA-256 code challenge method, as defined in [RFC 7636 §4.2](https://datatracker.ietf.org/doc/html/rfc7636#section-4.2)
const CODE_CHALLENGE_METHOD: &str = "S256";

/// The authorization URL and code verifier for an OAuth 2.0 PKCE flow
pub struct OAuthRequest {
  /// The authorization URL to open in the browser
  pub url: String,
  /// The code verifier to use in the token exchange after the user authorizes
  pub code_verifier: CodeVerifier,
}

/// Build an OAuth 2.0 authorization URL with a freshly generated PKCE code verifier
///
/// # Returns
/// 
/// An [`OAuthRequest`] containing the authorization URL and the code verifier
/// needed for the subsequent [`exchange_code`] call
pub fn get_oauth_url() -> OAuthRequest {
  let code_verifier = code_verifier::CodeVerifier::random();
  let url = Url::parse_with_params(
    URL_ENDPOINT,
    &[
      ("client_id", CLIENT_ID),
      ("response_type", RESPONSE_TYPE),
      ("scope", SCOPE),
      ("redirect_uri", REDIRECT_URI),
      ("code_challenge_method", CODE_CHALLENGE_METHOD),
      ("code_challenge", code_verifier.to_challenge().as_str()),
    ],
  )
  .expect("base URL is always valid");

  OAuthRequest {
    url: url.to_string(),
    code_verifier,
  }
}
