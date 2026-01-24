use crate::common::{MAGIC_PATCH, MAGIC_SIGNATURE, read_magic_bytes};
use crate::{Patch, Signature};

use std::io::BufRead;

pub enum WharfBinary<'a> {
  Signature(Signature<'a>),
  Patch(Patch<'a>),
}

impl WharfBinary<'_> {
  /// Dump the inner binary’s contents to standard output
  ///
  /// This delegates to [`Signature::dump_stdout`] or
  /// [`Patch::dump_stdout`], depending on the binary type.
  pub fn dump_stdout(&mut self) -> Result<(), String> {
    match self {
      WharfBinary::Signature(s) => s.dump_stdout(),
      WharfBinary::Patch(p) => p.dump_stdout(),
    }
  }

  /// Print a simple summary of the inner binary’s contents to standard output
  ///
  /// This delegates to [`Signature::print_summary`] or
  /// [`Patch::print_summary`], depending on the binary type.
  pub fn print_summary(&self) {
    match self {
      WharfBinary::Signature(s) => s.print_summary(),
      WharfBinary::Patch(p) => p.print_summary(),
    }
  }
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
