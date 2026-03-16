use rand::Rng;

/// A cryptographically random UUID version 4, as defined in
/// [RFC 9562 §5.4](https://datatracker.ietf.org/doc/html/rfc9562#section-5.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UuidV4(String);

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

    let uuid = format!(
      "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
      bytes[0],
      bytes[1],
      bytes[2],
      bytes[3],
      bytes[4],
      bytes[5],
      bytes[6],
      bytes[7],
      bytes[8],
      bytes[9],
      bytes[10],
      bytes[11],
      bytes[12],
      bytes[13],
      bytes[14],
      bytes[15]
    );

    Self(uuid)
  }

  /// Return the UUID as a string slice in the standard `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` format
  pub fn as_str(&self) -> &str {
    &self.0
  }
}
