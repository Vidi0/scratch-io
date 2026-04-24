#[derive(Clone, PartialEq, prost::Message)]
pub struct Container {
  #[prost(message, repeated, tag = "1")]
  pub files: Vec<File>,
  #[prost(message, repeated, tag = "2")]
  pub dirs: Vec<Dir>,
  #[prost(message, repeated, tag = "3")]
  pub symlinks: Vec<Symlink>,
  #[prost(int64, tag = "16")]
  pub size: i64,
}

#[derive(Clone, PartialEq, Eq, Hash, prost::Message)]
pub struct Dir {
  #[prost(string, tag = "1")]
  pub path: String,
  #[prost(uint32, tag = "2")]
  pub mode: u32,
}

#[derive(Clone, PartialEq, Eq, Hash, prost::Message)]
pub struct File {
  #[prost(string, tag = "1")]
  pub path: String,
  #[prost(uint32, tag = "2")]
  pub mode: u32,
  #[prost(int64, tag = "3")]
  pub size: i64,
  #[prost(int64, tag = "4")]
  pub offset: i64,
}

#[derive(Clone, PartialEq, Eq, Hash, prost::Message)]
pub struct Symlink {
  #[prost(string, tag = "1")]
  pub path: String,
  #[prost(uint32, tag = "2")]
  pub mode: u32,
  #[prost(string, tag = "3")]
  pub dest: String,
}
