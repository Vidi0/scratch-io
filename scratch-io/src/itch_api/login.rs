use super::LoginResponse;
use super::errors::{ItchRequestJSONError, LoginResponseError, TOTPResponseError};
use super::responses::TOTPResponse;
use super::types::LoginSuccess;
use super::{ItchApiUrl, ItchClient};

use reqwest::Method;

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
/// * `recaptcha_response` - A reCAPTCHA token from <https://itch.io/captcha>.
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
  recaptcha_response: &str,
) -> Result<LoginResponse, ItchRequestJSONError<LoginResponseError>> {
  client.itch_request_json::<LoginResponse>(&ItchApiUrl::v2("login"), Method::POST, |b| {
    b.form(&[
      ("username", username),
      ("password", password),
      // Even though the `force_recaptcha` parameter may suggest that the reCAPTCHA token
      // isn't always required, testing has shown that it is always required by the itch.io API.
      ("force_recaptcha", "true"),
      ("recaptcha_response", recaptcha_response),
      // source can be any of types::ItchKeySource
      ("source", "desktop"),
    ])
  })
}

/// Complete the login with the TOTP two-factor verification
///
/// The provided TOTP code must be either the six-digit code generated
/// by the TOTP application or one of the eight-digit recovery codes.
///
/// # Arguments
///
/// * `totp_token` - The TOTP token returned by the previous login step
///
/// * `totp_code` - The TOTP verification code, as explained avobe
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
    .itch_request_json::<TOTPResponse>(&ItchApiUrl::v2("totp/verify"), Method::POST, |b| {
      b.form(&[("token", totp_token), ("code", &totp_code.to_string())])
    })
    .map(|res| res.success)
}
