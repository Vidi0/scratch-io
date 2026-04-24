// Patch file format

#[derive(Clone, Copy, PartialEq, Eq, Hash, prost::Message)]
pub struct PatchHeader {
  #[prost(message, optional, tag = "1")]
  pub compression: Option<CompressionSettings>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, prost::Message)]
pub struct SyncHeader {
  #[prost(enumeration = "sync_header::Type", tag = "1")]
  pub r#type: i32,
  #[prost(int64, tag = "16")]
  pub file_index: i64,
}

/// Nested message and enum types in `SyncHeader`.
pub mod sync_header {
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
  #[repr(i32)]
  pub enum Type {
    Rsync = 0,
    /// when set, bsdiffTargetIndex must be set
    Bsdiff = 1,
  }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, prost::Message)]
pub struct BsdiffHeader {
  #[prost(int64, tag = "1")]
  pub target_index: i64,
}

#[derive(Clone, PartialEq, Eq, Hash, prost::Message)]
pub struct SyncOp {
  #[prost(enumeration = "sync_op::Type", tag = "1")]
  pub r#type: i32,
  #[prost(int64, tag = "2")]
  pub file_index: i64,
  #[prost(int64, tag = "3")]
  pub block_index: i64,
  #[prost(int64, tag = "4")]
  pub block_span: i64,
  #[prost(bytes = "vec", tag = "5")]
  pub data: Vec<u8>,
}

/// Nested message and enum types in `SyncOp`.
pub mod sync_op {
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
  #[repr(i32)]
  pub enum Type {
    BlockRange = 0,
    Data = 1,
    /// <3 @GranPC & @tomasduda
    HeyYouDidIt = 2049,
  }
}

// Signature file format

#[derive(Clone, Copy, PartialEq, Eq, Hash, prost::Message)]
pub struct SignatureHeader {
  #[prost(message, optional, tag = "1")]
  pub compression: Option<CompressionSettings>,
}

#[derive(Clone, PartialEq, Eq, Hash, prost::Message)]
pub struct BlockHash {
  #[prost(uint32, tag = "1")]
  pub weak_hash: u32,
  #[prost(bytes = "vec", tag = "2")]
  pub strong_hash: Vec<u8>,
}

// Compression

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum CompressionAlgorithm {
  None = 0,
  Brotli = 1,
  Gzip = 2,
  Zstd = 3,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, prost::Message)]
pub struct CompressionSettings {
  #[prost(enumeration = "CompressionAlgorithm", tag = "1")]
  pub algorithm: i32,
  #[prost(int32, tag = "2")]
  pub quality: i32,
}

// Manifest file format

#[derive(Clone, Copy, PartialEq, Eq, Hash, prost::Message)]
pub struct ManifestHeader {
  #[prost(message, optional, tag = "1")]
  pub compression: Option<CompressionSettings>,
  #[prost(enumeration = "HashAlgorithm", tag = "2")]
  pub algorithm: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum HashAlgorithm {
  Shake12832 = 0,
  Crc32c = 1,
}

#[derive(Clone, PartialEq, Eq, Hash, prost::Message)]
pub struct ManifestBlockHash {
  #[prost(bytes = "vec", tag = "1")]
  pub hash: Vec<u8>,
}

// Wounds file format

/// Wounds files format: header, container, then any
/// number of Wounds
#[derive(Clone, Copy, PartialEq, Eq, Hash, prost::Message)]
pub struct WoundsHeader {}

/// Describe a corrupted portion of a file, in [start,end)
#[derive(Clone, Copy, PartialEq, Eq, Hash, prost::Message)]
pub struct Wound {
  #[prost(int64, tag = "1")]
  pub index: i64,
  #[prost(int64, tag = "2")]
  pub start: i64,
  #[prost(int64, tag = "3")]
  pub end: i64,
  #[prost(enumeration = "WoundKind", tag = "4")]
  pub kind: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum WoundKind {
  File = 0,
  Symlink = 1,
  Dir = 2,
  /// sent when a file portion has been verified as valid
  ClosedFile = 3,
}
