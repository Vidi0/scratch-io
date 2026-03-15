//! PKCE (Proof Key for Code Exchange) implementation for OAuth 2.0 authorization flows.
//! See [RFC 7636](https://datatracker.ietf.org/doc/html/rfc7636).

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngExt;
use sha2::{Digest, Sha256};

/// Minimum length of a code verifier, as defined in
/// [RFC 7636 §4.1](https://datatracker.ietf.org/doc/html/rfc7636#section-4.1).
const CODE_VERIFIER_MIN_LEN: usize = 43;

/// Maximum length of a code verifier, as defined in
/// [RFC 7636 §4.1](https://datatracker.ietf.org/doc/html/rfc7636#section-4.1).
const CODE_VERIFIER_MAX_LEN: usize = 128;

/// Allowed characters for a code verifier (`A-Z`, `a-z`, `0-9`, and `-`, `.`, `_`, `~`), as
/// defined in [RFC 7636 §4.1](https://datatracker.ietf.org/doc/html/rfc7636#section-4.1).
const CODE_VERIFIER_CHARSET: &[char] = &[
  'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S',
  'T', 'U', 'V', 'W', 'X', 'Y', 'Z', // A-Z
  'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
  't', 'u', 'v', 'w', 'x', 'y', 'z', // a-z
  '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', // 0-9
  '-', // hyphen
  '.', // period
  '_', // underscore
  '~', // tilde
];

/// Generate a cryptographically random code verifier string of length between
/// [`CODE_VERIFIER_MIN_LEN`] and [`CODE_VERIFIER_MAX_LEN`], using the charset defined in
/// [RFC 7636 §4.1](https://datatracker.ietf.org/doc/html/rfc7636#section-4.1).
fn random_code_verifier() -> String {
  // Get a random length for the code verifier
  let mut rng = rand::rng();
  let len = rng.random_range(CODE_VERIFIER_MIN_LEN..=CODE_VERIFIER_MAX_LEN);

  // Fill the code with random characters from the charset
  let mut code_verifier = String::with_capacity(len);
  for _ in 0..len {
    let index = rng.random_range(0..CODE_VERIFIER_CHARSET.len());
    code_verifier.push(CODE_VERIFIER_CHARSET[index]);
  }

  code_verifier
}

/// Derive the code challenge from a code verifier by SHA-256 hashing it and encoding the result
/// as Base64-URL without padding, as defined in
/// [RFC 7636 §4.2](https://datatracker.ietf.org/doc/html/rfc7636#section-4.2).
fn code_challenge(code_verifier: &str) -> String {
  // Hash the code verifier
  let hash = Sha256::digest(code_verifier);

  // Encode it as base64
  URL_SAFE_NO_PAD.encode(hash)
}

/// A PKCE code verifier and its derived code challenge,
/// used in the OAuth 2.0 authorization flow.
/// See [RFC 7636](https://datatracker.ietf.org/doc/html/rfc7636).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeVerifier {
  verifier: String,
  challenge: String,
}

impl CodeVerifier {
  /// Generate a cryptographically random code verifier and
  /// derive its SHA-256 code challenge
  pub fn random() -> Self {
    let verifier = random_code_verifier();
    let challenge = code_challenge(&verifier);
    Self {
      verifier,
      challenge,
    }
  }

  pub fn verifier(&self) -> &str {
    &self.verifier
  }

  pub fn challenge(&self) -> &str {
    &self.challenge
  }
}
