//! UUID version 4 implementation as defined in
//! [RFC 9562 §5.4](https://datatracker.ietf.org/doc/html/rfc9562#section-5.4).

use rand::Rng;
use std::fmt::Display;

/// A cryptographically random UUID version 4, as defined in
/// [RFC 9562 §5.4](https://datatracker.ietf.org/doc/html/rfc9562#section-5.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UuidV4([u8; 16]);

impl UuidV4 {
  /// Generate a cryptographically random UUID v4, with the version bits set to `0100`
  /// and the variant bits set to `10xx`, as defined in
  /// [RFC 9562 §5.4](https://datatracker.ietf.org/doc/html/rfc9562#section-5.4).
  pub fn random() -> Self {
    // Generate 16 random bytes
    let mut rng = rand::rng();
    let mut bytes = [0u8; 16];
    rng.fill_bytes(&mut bytes);

    // Set version to 4 (0100xxxx)
    bytes[6] = (bytes[6] & 0b0000_1111) | 0b0100_0000;
    // Set variant to 10xx (10xxxxxx)
    bytes[8] = (bytes[8] & 0b0011_1111) | 0b1000_0000;

    Self(bytes)
  }
}

impl Display for UuidV4 {
  /// Formats the UUID as a lowercase hyphenated string: `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
      self.0[0],
      self.0[1],
      self.0[2],
      self.0[3],
      self.0[4],
      self.0[5],
      self.0[6],
      self.0[7],
      self.0[8],
      self.0[9],
      self.0[10],
      self.0[11],
      self.0[12],
      self.0[13],
      self.0[14],
      self.0[15]
    )
  }
}
