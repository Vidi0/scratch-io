use crate::binaries::manifest::Manifest;
use crate::binaries::patch::Patch;
use crate::binaries::signature::Signature;
use crate::binaries::wounds::Wounds;
use crate::binaries::zip_index::ZipIndex;
use crate::binaries::{Dump, WharfBinary};
use crate::errors::{InvalidBinary, Result};
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
  Patch(Patch<'reader, R>),
  Signature(Signature<'reader, R>),
  Manifest(Manifest<'reader, R>),
  Wounds(Wounds<'reader, R>),
  ZipIndex(ZipIndex<'reader, R>),
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
      magic => return Err(InvalidBinary::MagicNotFound { found: magic }.into()),
    })
  }

  /// `reader` must *have* consumed its magic bytes
  pub fn read<R: BufRead>(self, reader: &mut R) -> Result<WharfBinaryRead<'_, R>> {
    Ok(match self {
      Self::Patch => WharfBinaryRead::Patch(Patch::read_without_magic(reader)?),
      Self::Signature => WharfBinaryRead::Signature(Signature::read_without_magic(reader)?),
      Self::Manifest => WharfBinaryRead::Manifest(Manifest::read_without_magic(reader)?),
      Self::Wounds => WharfBinaryRead::Wounds(Wounds::read_without_magic(reader)?),
      Self::ZipIndex => WharfBinaryRead::ZipIndex(ZipIndex::read_without_magic(reader)?),
    })
  }
}

impl<R: BufRead> Display for WharfBinaryRead<'_, R> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Patch(patch) => write!(f, "{patch}"),
      Self::Signature(signature) => write!(f, "{signature}"),
      Self::Manifest(manifest) => write!(f, "{manifest}"),
      Self::Wounds(wounds) => write!(f, "{wounds}"),
      Self::ZipIndex(zip_index) => write!(f, "{zip_index}"),
    }
  }
}

impl<R: BufRead> Dump for WharfBinaryRead<'_, R> {
  fn dump(&mut self, writer: &mut impl std::io::Write) -> Result<()> {
    match self {
      Self::Patch(patch) => patch.dump(writer),
      Self::Signature(signature) => signature.dump(writer),
      Self::Manifest(manifest) => manifest.dump(writer),
      Self::Wounds(wounds) => wounds.dump(writer),
      Self::ZipIndex(zip_index) => zip_index.dump(writer),
    }
  }
}
