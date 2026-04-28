/// <https://github.com/itchio/wharf/blob/5e5efc838cdbaee7915246d5102af78a3a31e74d/bsdiff/bsdiff.proto>
///
/// More information about bsdiff wharf patches:
/// <https://web.archive.org/web/20211123032456/https://twitter.com/fasterthanlime/status/790617515009437701>
mod bsdiff;
/// <https://github.com/itchio/wharf/blob/5e5efc838cdbaee7915246d5102af78a3a31e74d/pwr/pwr.proto>
mod pwr;
/// <https://github.com/itchio/lake/blob/d93a9d33bb65f76200e07d9606e1e251fd09cb07/tlc/tlc.proto>
mod tlc;

use super::Message;
use crate::errors::{Error, InvalidWharfMessage, Result};

// Helper functions

fn try_i64_into_u64<MessageType>(value: i64) -> Result<u64> {
  value
    .try_into()
    .map_err(|_| InvalidWharfMessage::ExpectedU64 { int: value }.into_error::<MessageType>())
}

fn try_i64_into_usize<MessageType>(value: i64) -> Result<usize> {
  value
    .try_into()
    .map_err(|_| InvalidWharfMessage::ExpectedUsize { int: value }.into_error::<MessageType>())
}

fn try_unwrap_option<MessageType, T>(value: Option<T>, field_name: &'static str) -> Result<T> {
  value.ok_or_else(|| {
    InvalidWharfMessage::MissingProtoField { field_name }.into_error::<MessageType>()
  })
}

// Compression

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompressionAlgorithm {
  None,
  Brotli,
  Gzip,
  Zstd,
}

impl From<pwr::CompressionAlgorithm> for CompressionAlgorithm {
  fn from(value: pwr::CompressionAlgorithm) -> Self {
    match value {
      pwr::CompressionAlgorithm::None => Self::None,
      pwr::CompressionAlgorithm::Brotli => Self::Brotli,
      pwr::CompressionAlgorithm::Gzip => Self::Gzip,
      pwr::CompressionAlgorithm::Zstd => Self::Zstd,
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CompressionSettings {
  pub algorithm: CompressionAlgorithm,
  pub quality: i32,
}

impl From<pwr::CompressionSettings> for CompressionSettings {
  fn from(value: pwr::CompressionSettings) -> Self {
    Self {
      algorithm: value.algorithm().into(),
      quality: value.quality,
    }
  }
}

// Patch file format

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PatchHeader {
  pub compression: CompressionSettings,
}

impl TryFrom<pwr::PatchHeader> for PatchHeader {
  type Error = Error;

  fn try_from(value: pwr::PatchHeader) -> Result<Self> {
    Ok(Self {
      compression: try_unwrap_option::<Self, _>(value.compression, "compression")?.into(),
    })
  }
}

impl Message for PatchHeader {
  type ProtoMessage = pwr::PatchHeader;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SyncHeader {
  pub r#type: sync_header::Type,
  pub file_index: usize,
}

impl TryFrom<pwr::SyncHeader> for SyncHeader {
  type Error = Error;

  fn try_from(value: pwr::SyncHeader) -> Result<Self> {
    Ok(Self {
      r#type: value.r#type().into(),
      file_index: try_i64_into_usize::<Self>(value.file_index)?,
    })
  }
}

impl Message for SyncHeader {
  type ProtoMessage = pwr::SyncHeader;
}

/// Nested message and enum types in `SyncHeader`.
pub mod sync_header {
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
  pub enum Type {
    Rsync,
    Bsdiff,
  }

  impl From<super::pwr::sync_header::Type> for Type {
    fn from(value: super::pwr::sync_header::Type) -> Self {
      match value {
        super::pwr::sync_header::Type::Rsync => Self::Rsync,
        super::pwr::sync_header::Type::Bsdiff => Self::Bsdiff,
      }
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BsdiffHeader {
  pub target_index: usize,
}

impl TryFrom<pwr::BsdiffHeader> for BsdiffHeader {
  type Error = Error;

  fn try_from(value: pwr::BsdiffHeader) -> Result<Self> {
    Ok(Self {
      target_index: try_i64_into_usize::<Self>(value.target_index)?,
    })
  }
}

impl Message for BsdiffHeader {
  type ProtoMessage = pwr::BsdiffHeader;
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SyncOp {
  BlockRange {
    file_index: usize,
    block_index: u64,
    block_span: u64,
  },
  Data(Box<[u8]>),
  HeyYouDidIt,
}

impl TryFrom<pwr::SyncOp> for SyncOp {
  type Error = Error;

  fn try_from(value: pwr::SyncOp) -> Result<Self> {
    Ok(match value.r#type() {
      pwr::sync_op::Type::BlockRange => Self::BlockRange {
        file_index: try_i64_into_usize::<Self>(value.file_index)?,
        block_index: try_i64_into_u64::<Self>(value.block_index)?,
        block_span: try_i64_into_u64::<Self>(value.block_span)?,
      },
      pwr::sync_op::Type::Data => Self::Data(value.data.into_boxed_slice()),
      pwr::sync_op::Type::HeyYouDidIt => Self::HeyYouDidIt,
    })
  }
}

impl Message for SyncOp {
  type ProtoMessage = pwr::SyncOp;
}

/// Control is a bsdiff operation, see <https://twitter.com/fasterthanlime/status/790617515009437701>
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Control {
  Op {
    add: Box<[u8]>,
    copy: Box<[u8]>,
    seek: i64,
  },
  Eof,
}

impl From<bsdiff::Control> for Control {
  fn from(value: bsdiff::Control) -> Self {
    if value.eof {
      Self::Eof
    } else {
      Self::Op {
        add: value.add.into_boxed_slice(),
        copy: value.copy.into_boxed_slice(),
        seek: value.seek,
      }
    }
  }
}

impl Message for Control {
  type ProtoMessage = bsdiff::Control;
}

// Signature file format

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SignatureHeader {
  pub compression: CompressionSettings,
}

impl TryFrom<pwr::SignatureHeader> for SignatureHeader {
  type Error = Error;

  fn try_from(value: pwr::SignatureHeader) -> Result<Self> {
    Ok(Self {
      compression: try_unwrap_option::<Self, _>(value.compression, "compression")?.into(),
    })
  }
}

impl Message for SignatureHeader {
  type ProtoMessage = pwr::SignatureHeader;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockHash {
  pub weak_hash: u32,
  pub strong_hash: [u8; 16],
}

impl TryFrom<pwr::BlockHash> for BlockHash {
  type Error = Error;

  fn try_from(value: pwr::BlockHash) -> Result<Self> {
    let strong_hash: [u8; 16] = match value.strong_hash.try_into() {
      Ok(hash) => hash,
      Err(vec) => {
        return Err(
          InvalidWharfMessage::ExpectedVecLength {
            expected: 16,
            found: vec.len(),
          }
          .into_error::<Self>(),
        );
      }
    };

    Ok(Self {
      weak_hash: value.weak_hash,
      strong_hash,
    })
  }
}

impl Message for BlockHash {
  type ProtoMessage = pwr::BlockHash;
}

// Manifest file format

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ManifestHeader {
  pub compression: CompressionSettings,
  pub algorithm: HashAlgorithm,
}

impl TryFrom<pwr::ManifestHeader> for ManifestHeader {
  type Error = Error;

  fn try_from(value: pwr::ManifestHeader) -> Result<Self> {
    Ok(Self {
      compression: try_unwrap_option::<Self, _>(value.compression, "compression")?.into(),
      algorithm: value.algorithm().into(),
    })
  }
}

impl Message for ManifestHeader {
  type ProtoMessage = pwr::ManifestHeader;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HashAlgorithm {
  Shake12832,
  Crc32c,
}

impl From<pwr::HashAlgorithm> for HashAlgorithm {
  fn from(value: pwr::HashAlgorithm) -> Self {
    match value {
      pwr::HashAlgorithm::Shake12832 => Self::Shake12832,
      pwr::HashAlgorithm::Crc32c => Self::Crc32c,
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ManifestBlockHash {
  pub hash: Vec<u8>,
}

impl From<pwr::ManifestBlockHash> for ManifestBlockHash {
  fn from(value: pwr::ManifestBlockHash) -> Self {
    Self { hash: value.hash }
  }
}

impl Message for ManifestBlockHash {
  type ProtoMessage = pwr::ManifestBlockHash;
}

// Wounds file format

/// Wounds files format: header, container, then any
/// number of Wounds
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WoundsHeader;

impl From<pwr::WoundsHeader> for WoundsHeader {
  fn from(_value: pwr::WoundsHeader) -> Self {
    Self
  }
}

impl Message for WoundsHeader {
  type ProtoMessage = pwr::WoundsHeader;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[expect(dead_code)]
pub enum Wound {
  File {
    index: usize,
    start: u64,
    end: u64,
  },
  Symlink {
    index: usize,
  },
  Dir {
    index: usize,
  },
  /// sent when a file portion has been verified as valid
  ClosedFile,
}

impl TryFrom<pwr::Wound> for Wound {
  type Error = Error;

  fn try_from(_value: pwr::Wound) -> Result<Self> {
    todo!()
  }
}

// Container

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Container {
  pub files: Vec<File>,
  pub dirs: Vec<Dir>,
  pub symlinks: Vec<Symlink>,
  pub size: u64,
}

impl TryFrom<tlc::Container> for Container {
  type Error = Error;

  fn try_from(value: tlc::Container) -> Result<Self> {
    let files = value
      .files
      .into_iter()
      .map(TryInto::try_into)
      .collect::<Result<Vec<File>>>()?;

    let dirs = value.dirs.into_iter().map(Into::into).collect();
    let symlinks = value.symlinks.into_iter().map(Into::into).collect();

    Ok(Container {
      files,
      dirs,
      symlinks,
      size: try_i64_into_u64::<Self>(value.size)?,
    })
  }
}

impl Message for Container {
  type ProtoMessage = tlc::Container;
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Dir {
  pub path: String,
  pub mode: u32,
}

impl From<tlc::Dir> for Dir {
  fn from(value: tlc::Dir) -> Self {
    Self {
      path: value.path,
      mode: value.mode,
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct File {
  pub path: String,
  pub mode: u32,
  pub size: u64,
  pub offset: u64,
}

impl TryFrom<tlc::File> for File {
  type Error = Error;

  fn try_from(value: tlc::File) -> Result<Self> {
    Ok(Self {
      path: value.path,
      mode: value.mode,
      size: try_i64_into_u64::<Self>(value.size)?,
      offset: try_i64_into_u64::<Self>(value.offset)?,
    })
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Symlink {
  pub path: String,
  pub mode: u32,
  pub dest: String,
}

impl From<tlc::Symlink> for Symlink {
  fn from(value: tlc::Symlink) -> Self {
    Self {
      path: value.path,
      mode: value.mode,
      dest: value.dest,
    }
  }
}
