pub mod apply;

mod read;

use crate::protos::{pwr, tlc};

use std::io::BufRead;

/// Represents a decoded wharf patch file
///
/// <https://docs.itch.zone/wharf/master/file-formats/patches.html>
///
/// Contains the header, the old and new containers describing file system
/// state before and after the patch, and an iterator over the patch operations.
/// The iterator reads from the underlying stream on the fly as items are requested.
pub struct Patch<'a> {
  pub header: pwr::PatchHeader,
  pub container_old: tlc::Container,
  pub container_new: tlc::Container,
  pub sync_op_iter: SyncEntryIter<Box<dyn BufRead + 'a>>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RsyncOpIter<'a, R> {
  reader: &'a mut R,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BsdiffOpIter<'a, R> {
  reader: &'a mut R,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SyncHeader<'a, R> {
  pub file_index: i64,
  pub kind: SyncHeaderKind<'a, R>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SyncHeaderKind<'a, R> {
  Rsync {
    op_iter: RsyncOpIter<'a, R>,
  },
  Bsdiff {
    target_index: i64,
    op_iter: BsdiffOpIter<'a, R>,
  },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncEntryIter<R> {
  reader: R,
  total_entries: u64,
  entries_read: u64,
}

impl<R> SyncEntryIter<R> {
  #[must_use]
  pub const fn total_entries(&self) -> u64 {
    self.total_entries
  }
}
