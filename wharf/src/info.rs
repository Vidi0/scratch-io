use crate::common::{MAGIC_PATCH, MAGIC_SIGNATURE, read_magic_bytes};
use crate::{Patch, Signature};

use std::io::BufRead;

pub enum WharfBinary<'a> {
  Signature(Signature<'a>),
  Patch(Patch<'a>),
}

/// Itentify a wharf binary based on the magic bytes and decode it
pub fn identify<'a>(reader: &'a mut impl BufRead) -> Result<WharfBinary<'a>, String> {
  use WharfBinary as WB;

  let magic = read_magic_bytes(reader)?;
  match magic {
    MAGIC_SIGNATURE => Ok(WB::Signature(Signature::read_without_magic(reader)?)),
    MAGIC_PATCH => Ok(WB::Patch(Patch::read_without_magic(reader)?)),
    _ => Err("The provided binary doesn't match with any known wharf binary format".to_string()),
  }
}
