use super::{ItchClient, types::ItchApiUrl};

use bytes::Bytes;
use futures_util::{FutureExt, Stream, StreamExt};
use reqwest::{Method, Response, header};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

enum ReaderState<'a> {
  WaitingForData(Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Unpin + Send + 'a>>),
  WaitingForResponse(Pin<Box<dyn Future<Output = Result<Response, reqwest::Error>> + Send + 'a>>),
}

pub struct HttpSeekReader<'a> {
  /// An itch.io API client to make the requests
  client: &'a ItchClient,
  /// The URL where the requests will be made
  url: Arc<ItchApiUrl>,
  /// The total length of the data to be downloaded
  len: u64,
  /// The current absolute position of the reader in the data
  pos: u64,
  /// The current response state
  state: ReaderState<'a>,
}

impl<'a> HttpSeekReader<'a> {
  pub fn new(client: &'a ItchClient, url: ItchApiUrl, len: u64) -> Self {
    let url = Arc::new(url);

    Self {
      client,
      len,
      pos: 0,
      state: ReaderState::WaitingForResponse(Box::pin(client.itch_request(
        url.clone(),
        Method::GET,
        |b| b,
      ))),
      url,
    }
  }

  fn new_request(&mut self, start: u64) {
    // Change the internal bytes offsets to the new positions
    self.pos = start;

    // Change the internal reader's state
    self.state = ReaderState::WaitingForResponse(Box::pin(self.client.itch_request(
      self.url.clone(),
      Method::GET,
      move |b| b.header(header::RANGE, format!("bytes={start}-")),
    )));
  }
}

impl<'a> tokio::io::AsyncRead for HttpSeekReader<'a> {
  fn poll_read(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut tokio::io::ReadBuf<'_>,
  ) -> Poll<std::io::Result<()>> {
    // Get a mutable reference to self
    let this = self.get_mut();

    match &mut this.state {
      // If the reader already has an stream of data, read from it
      ReaderState::WaitingForData(data) => match data.poll_next_unpin(cx) {
        // If the data is ready, read it
        Poll::Ready(Some(Ok(bytes))) => {
          // The length of the data must not exceed the remaining capacity of the buffer.
          let len = std::cmp::min(bytes.len(), buf.remaining());

          // Put the first len bytes into the buffer (to prevent overflows)
          buf.put_slice(&bytes.slice(..len));

          // Move the internal data pointer
          this.pos += len as u64;

          Poll::Ready(Ok(()))
        }

        // If an error was encountered, return it
        Poll::Ready(Some(Err(e))) => {
          Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other, e)))
        }

        // If the poll is ready, but does not contain data, skip it
        Poll::Ready(None) => Poll::Ready(Ok(())),

        // If the future isn't ready yet, return Pending
        Poll::Pending => Poll::Pending,
      },

      // If the reader is waiting for an API request, poll it
      ReaderState::WaitingForResponse(res) => match res.poll_unpin(cx) {
        // If the response is ready
        Poll::Ready(Ok(res)) => {
          // Create a bytes stream from the response body
          let bytes_stream = res.bytes_stream();

          // Replace the current state with WaitingForData
          this.state = ReaderState::WaitingForData(Box::pin(bytes_stream));

          // Return Pending so the executor can wake this task again and continue reading from the new data stream
          Poll::Pending
        }

        // If an error was encountered, return it
        Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other, e))),

        // If the future isn't ready yet, return Pending
        Poll::Pending => Poll::Pending,
      },
    }
  }
}

impl<'a> tokio::io::AsyncBufRead for HttpSeekReader<'a> {
  fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
    // Get a mutable reference to self
    let this = self.get_mut();

    match &mut this.state {
      // If the reader already has an stream of data, read from it
      ReaderState::WaitingForData(data) => match data.poll_next_unpin(cx) {
        // If the data is ready, read it
        Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Ok(&bytes)),

        // If an error was encountered, return it
        Poll::Ready(Some(Err(e))) => {
          Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other, e)))
        }

        // If the poll is ready, but does not contain data, skip it
        Poll::Ready(None) => Poll::Ready(Ok(&[])),

        // If the future isn't ready yet, return Pending
        Poll::Pending => Poll::Pending,
      },

      // If the reader is waiting for an API request, poll it
      ReaderState::WaitingForResponse(res) => match res.poll_unpin(cx) {
        // If the response is ready
        Poll::Ready(Ok(res)) => {
          // Create a bytes stream from the response body
          let bytes_stream = res.bytes_stream();

          // Replace the current state with WaitingForData
          this.state = ReaderState::WaitingForData(Box::pin(bytes_stream));

          // Return Pending so the executor can wake this task again and continue reading from the new data stream
          Poll::Pending
        }

        // If an error was encountered, return it
        Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other, e))),

        // If the future isn't ready yet, return Pending
        Poll::Pending => Poll::Pending,
      },
    }
  }

  fn consume(self: Pin<&mut Self>, amt: usize) {}
}

impl<'a> tokio::io::AsyncSeek for HttpSeekReader<'a> {
  fn start_seek(self: Pin<&mut Self>, position: std::io::SeekFrom) -> std::io::Result<()> {
    // Get a mutable reference to self
    let this = self.get_mut();

    // Allow seeks beyond the end of the stream at this point
    let new_position = match position {
      // When seeking for a specific byte number, set the position to that offset
      std::io::SeekFrom::Start(new_pos) => new_pos,

      // When seeking for a specific byte number relative to the end
      // Set the position to the end of the file plus the offset
      std::io::SeekFrom::End(offset) => {
        // Get the new absolute position
        let Some(new_pos) = this.len.checked_add_signed(offset) else {
          return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Seek before start of the stream or overflow happened",
          ));
        };

        new_pos
      }

      // Seek for an offset relative to the current position
      std::io::SeekFrom::Current(offset) => {
        // Get the new absolute position
        let Some(new_pos) = this.pos.checked_add_signed(offset) else {
          return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Seek before start of the stream or overflow happened",
          ));
        };

        new_pos
      }
    };

    // If the new position is beyond the end of the stream, return an error
    if new_position > this.len {
      return Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "Seek beyond end of the stream",
      ));
    }

    // Change the internal pointer
    this.pos = new_position;

    // Request more data at the given position
    this.new_request(new_position);

    Ok(())
  }

  fn poll_complete(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<u64>> {
    let this = self.get_mut();
    Poll::Ready(Ok(this.pos))
  }
}
