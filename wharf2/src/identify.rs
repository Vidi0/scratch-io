use crate::binaries::signature::Signature;
use crate::binaries::wounds::Wounds;
use crate::binaries::{Dump, WharfBinary};
use crate::errors::{InvalidWharfBinary, Result};
use crate::magic::{
  MANIFEST_MAGIC, PATCH_MAGIC, SIGNATURE_MAGIC, WOUNDS_MAGIC, ZIP_INDEX_MAGIC, read_magic_bytes,
};

use std::fmt::Display;
use std::io::{BufRead, Read};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WharfBinaryKind {
  Patch,
  Signature,
  Manifest,
  Wounds,
  ZipIndex,
}

pub enum WharfBinaryRead<'reader, R: BufRead> {
  Signature(Signature<'reader, R>),
  Wounds(Wounds<'reader, R>),
}

impl WharfBinaryKind {
  /// `reader` must *not* have consumed its magic bytes yet
  ///
  /// After this call, only the magic bytes will be consumed from the reader
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

  /// `reader` must *have* consumed its magic bytes
  ///
  /// # Panics
  ///
  /// The following binaries are not implemented yet: [`Self::Patch`], [`Self::Manifest`]
  /// and [`Self::ZipIndex`].
  ///
  /// Therefore, calling this function on any of those variants will panic with a todo message.
  pub fn read<R: BufRead>(self, reader: &mut R) -> Result<WharfBinaryRead<'_, R>> {
    Ok(match self {
      Self::Patch => todo!(),
      Self::Signature => WharfBinaryRead::Signature(Signature::read_without_magic(reader)?),
      Self::Manifest => todo!(),
      Self::Wounds => WharfBinaryRead::Wounds(Wounds::read_without_magic(reader)?),
      Self::ZipIndex => todo!(),
    })
  }
}

impl<R: BufRead> Dump for WharfBinaryRead<'_, R> {
  fn dump(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
    match self {
      Self::Signature(signature) => signature.dump(writer),
      Self::Wounds(wounds) => wounds.dump(writer),
    }
  }
}

impl<R: BufRead> Display for WharfBinaryRead<'_, R> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Signature(signature) => write!(f, "{signature}"),
      Self::Wounds(wounds) => write!(f, "{wounds}"),
    }
  }
}
