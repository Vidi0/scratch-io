use crate::errors::{NetworkError, NetworkErrorKind as NetErr};

/// [`futures_util::stream::StreamExt::next`]
pub async fn next_chunk<T>(
  stream: &mut (impl futures_util::Stream<Item = Result<T, reqwest::Error>> + Unpin),
) -> Result<Option<T>, NetworkError> {
  use futures_util::StreamExt;

  match stream.next().await {
    None => Ok(None),
    Some(result) => result.map(Some).map_err(NetErr::CouldntReadChunk.attach()),
  }
}
