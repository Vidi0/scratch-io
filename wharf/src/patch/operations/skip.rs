use crate::patch::{OpIter, RsyncOp, SyncHeader, SyncHeaderKind, op_kind};
use crate::pool::{ContainerBackedPool, SeekablePool};

use std::io::Read;
use std::iter;

#[derive(Debug)]
#[must_use]
pub enum SkipStatus<'reader, R> {
  /// The file uses bsdiff and cannot be skipped
  /// (the patch operation always represents actual changes in the file)
  NotSkippableBsdiff {
    target_index: usize,
    op_iter: OpIter<'reader, R, op_kind::Bsdiff>,
  },

  /// The file uses rsync and cannot be skipped
  ///
  /// The first rsync operation has already been consumed from the iterator,
  /// so it is chained to the start of op_iter to allow it to be applied before
  /// continuing with the remaining operations.
  ///
  /// The returned iterator will iterate over all the rsync patch operations
  NotSkippableRsync {
    op_iter: iter::Chain<iter::Once<Result<RsyncOp, String>>, OpIter<'reader, R, op_kind::Rsync>>,
  },

  /// The file is a literal copy of an old file at the given index
  LiteralCopy { old_index: usize },

  /// The file is empty
  Empty,
}

impl<'reader, R: Read> SyncHeader<'reader, R> {
  /// Check if the new file needs to be patched, or if it can be skipped/is empty
  ///
  /// Rsync operations can be used to determine two special cases:
  ///
  /// 1. The new file is a literal copy of one in the old container
  /// 2. The new file is empty
  ///
  /// For that reason, check if the *first* operation represents
  /// one of these special cases.
  ///
  /// The checkpoint must *NOT* exist for this file, because a checkpoint
  /// means patching actually started
  pub fn check_skip(
    self,
    new_file_size: u64,
    src_pool: &mut (impl SeekablePool + ContainerBackedPool),
  ) -> Result<SkipStatus<'reader, R>, String> {
    let mut op_iter = match self.kind {
      SyncHeaderKind::Rsync { op_iter } => op_iter,
      // If the kind is bsdiff, return the iterator and target index
      SyncHeaderKind::Bsdiff {
        target_index,
        op_iter,
      } => {
        return Ok(SkipStatus::NotSkippableBsdiff {
          target_index,
          op_iter,
        });
      }
    };

    // Get the first patch operation
    let first_op = match op_iter.next() {
      Some(op) => op?,
      None => {
        return Err(
          "Expected the first rsync SyncOp for this file, but received None?".to_string(),
        );
      }
    };

    Ok(
      // Check if the new file is empty
      if first_op.is_empty_file(new_file_size) {
        SkipStatus::Empty
      }
      // Check if the new file is a literal copy of one in the old container
      else if let Some(old_index) = first_op.is_literal_copy(new_file_size, src_pool)? {
        SkipStatus::LiteralCopy { old_index }
      }
      // Else, the file will have to be patched, so chain the first operation
      // (which has been obtained independently) into the op_iter patch iterator
      else {
        SkipStatus::NotSkippableRsync {
          op_iter: iter::chain(iter::once(Ok(first_op)), op_iter),
        }
      },
    )
  }
}
