use crate::errors::{InvalidWharfBinary, Result};
use crate::magic::{
  MANIFEST_MAGIC, PATCH_MAGIC, SIGNATURE_MAGIC, WOUNDS_MAGIC, ZIP_INDEX_MAGIC, read_magic_bytes,
};

use std::io::Read;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WharfBinaryKind {
  Patch,
  Signature,
  Manifest,
  Wounds,
  ZipIndex,
}

impl WharfBinaryKind {
  /// `reader` must have not consumed its magic bytes yet
  pub fn identify<R: Read>(reader: &mut R) -> Result<Self> {
    Ok(match read_magic_bytes(reader)? {
      PATCH_MAGIC => Self::Patch,
      SIGNATURE_MAGIC => Self::Signature,
      MANIFEST_MAGIC => Self::Manifest,
      WOUNDS_MAGIC => Self::Wounds,
      ZIP_INDEX_MAGIC => Self::ZipIndex,
      magic => return Err(InvalidWharfBinary::MagicNotFound { found: magic }.into()),
    })
  }
}
