mod code_verifier;

pub use code_verifier::CodeVerifier;

/// OAuth client ID used by the itch.io APP
const CLIENT_ID: &str = "85252daf268d27fbefac93e1ac462bfd";
const REDIRECT_URI: &str = "itch%3A%2F%2Foauth-callback";
const URL_ENDPOINT: &str = "https://itch.io/user/oauth";

pub fn get_oauth_url() -> String {
  let code_verifier = code_verifier::CodeVerifier::random();

  format!(
    "{URL_ENDPOINT}\
    ?client_id={CLIENT_ID}\
    &scope=itch\
    &redirect_uri={REDIRECT_URI}\
    &response_type=code\
    &code_challenge={}\
    &code_challenge_method=S256",
    code_verifier.challenge()
  )
}
