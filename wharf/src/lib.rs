/// Wharf protobuf definitions
pub mod protos;

/// Funcions and structures for reading wharf patches
pub mod patch;
/// Funcions and structures for reading wharf signatures
pub mod signature;

/// Identify the kind of wharf binary provided
pub mod info;

mod common;
mod container;
mod hasher;

pub use patch::read::Patch;
pub use signature::read::Signature;

pub use common::{PATCH_MAGIC, SIGNATURE_MAGIC};

pub use container::BLOCK_SIZE;
