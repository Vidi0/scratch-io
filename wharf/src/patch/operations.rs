pub mod apply;
pub mod skip;

mod bsdiff;
mod rsync;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum OpStatus {
  Ok { written_bytes: u64 },
  Broken,
}
