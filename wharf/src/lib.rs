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

pub use patch::Patch;
pub use signature::Signature;

pub use common::{MAGIC_PATCH, MAGIC_SIGNATURE};

pub use container::BLOCK_SIZE;
