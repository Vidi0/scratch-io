//! PKCE (Proof Key for Code Exchange) implementation for OAuth 2.0 authorization flows.
//! See [RFC 7636](https://datatracker.ietf.org/doc/html/rfc7636).

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng;
use sha2::{Digest, Sha256};
use std::fmt::Display;

/// A cryptographically random PKCE code verifier, as defined in
/// [RFC 7636 §4.1](https://datatracker.ietf.org/doc/html/rfc7636#section-4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeVerifier(String);

/// A PKCE code challenge derived from a [`CodeVerifier`], as defined in
/// [RFC 7636 §4.2](https://datatracker.ietf.org/doc/html/rfc7636#section-4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeChallenge(String);

impl CodeVerifier {
  /// Generate a cryptographically random code verifier as defined in
  /// [RFC 7636 §4.1](https://datatracker.ietf.org/doc/html/rfc7636#section-4.1).
  pub fn random(rng: &mut impl Rng) -> Self {
    // Generate 32 random bytes and encode them as base64url,
    // following the recommendation in RFC 7636 §4.1
    let mut bytes = [0u8; 32];
    rng.fill_bytes(&mut bytes);

    Self(URL_SAFE_NO_PAD.encode(bytes))
  }

  /// Derive the code challenge from a code verifier by SHA-256 hashing it and encoding the result
  /// as Base64-URL without padding, as defined in
  /// [RFC 7636 §4.2](https://datatracker.ietf.org/doc/html/rfc7636#section-4.2).
  pub fn to_challenge(&self) -> CodeChallenge {
    let hash = Sha256::digest(&self.0);
    CodeChallenge(URL_SAFE_NO_PAD.encode(hash))
  }
}

impl Display for CodeVerifier {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.0)
  }
}

impl Display for CodeChallenge {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.0)
  }
}
