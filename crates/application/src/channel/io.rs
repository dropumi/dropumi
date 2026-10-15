// SPDX-License-Identifier: MPL-2.0
//! Encrypted record I/O over any byte stream.
//!
//! Buffers are allocated once per session and reused for every record, so
//! memory per session is bounded (~256 KiB) regardless of file size.

use std::fmt;
use std::time::Duration;

use dropumi_domain::error::{Phase, TransferError};
use dropumi_protocol::ProtocolError;
use dropumi_protocol::frame::Frame;
use dropumi_protocol::limits::{MAX_FRAME_BODY, MAX_PLAINTEXT, NOISE_MAX_MSG};
use dropumi_protocol::message::{Message, encode};
use dropumi_protocol::record::{LEN_PREFIX, MAX_RECORD, Opener, RecordPhase, Sealer, parse_len};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;

use crate::error::map_io;

/// Why receiving failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecvError {
    /// The connection failed, closed or timed out.
    Transport(TransferError),
    /// The peer sent something invalid (send it an `Error`).
    Protocol(ProtocolError),
}

impl From<RecvError> for TransferError {
    fn from(e: RecvError) -> Self {
        match e {
            RecvError::Transport(t) => t,
            RecvError::Protocol(p) => p.to_transfer_error(),
        }
    }
}

impl From<ProtocolError> for RecvError {
    fn from(e: ProtocolError) -> Self {
        Self::Protocol(e)
    }
}

/// Reads and decrypts records.
pub struct RecordReader<R> {
    inner: R,
    opener: Opener,
    cipher: Box<[u8]>,
    plain: Box<[u8]>,
    idle: Duration,
    /// Bytes of the current record read so far (prefix included); kept
    /// across calls so that [`RecordReader::recv`] is cancel-safe.
    filled: usize,
    /// Body length once the prefix is complete.
    body_len: Option<usize>,
}

impl<R> fmt::Debug for RecordReader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecordReader")
            .field("opener", &self.opener)
            .finish_non_exhaustive()
    }
}

impl<R: AsyncRead + Unpin> RecordReader<R> {
    pub(crate) fn new(inner: R, opener: Opener, idle: Duration) -> Self {
        Self {
            inner,
            opener,
            cipher: vec![0u8; MAX_RECORD].into_boxed_slice(),
            plain: vec![0u8; NOISE_MAX_MSG].into_boxed_slice(),
            idle,
            filled: 0,
            body_len: None,
        }
    }

    /// The next frame, waiting at most the idle timeout.
    ///
    /// # Errors
    /// [`RecvError`]; end of stream is `ConnectionLost`.
    pub async fn recv(&mut self, phase: Phase) -> Result<Frame<'_>, RecvError> {
        let limit = self.idle;
        self.recv_within(limit, phase).await
    }

    /// The next frame, waiting at most `limit`.
    ///
    /// # Errors
    /// [`RecvError`]; end of stream is `ConnectionLost`.
    pub async fn recv_within(
        &mut self,
        limit: Duration,
        phase: Phase,
    ) -> Result<Frame<'_>, RecvError> {
        let n = match timeout(limit, self.read_record()).await {
            Err(_) => return Err(RecvError::Transport(TransferError::Timeout(phase))),
            Ok(r) => r?,
        };
        self.filled = 0;
        self.body_len = None;
        let ct = self
            .cipher
            .get(LEN_PREFIX..LEN_PREFIX.saturating_add(n))
            .ok_or(RecvError::Protocol(ProtocolError::Decrypt))?;
        let plain = self.opener.open(ct, &mut self.plain)?;
        Ok(Frame::decode(plain)?)
    }

    /// Reads and discards input until end of stream, an error or `limit`.
    pub async fn drain(&mut self, limit: Duration) {
        let mut sink = [0u8; 4096];
        let _ = timeout(limit, async {
            loop {
                match self.inner.read(&mut sink).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        })
        .await;
    }
}

impl<R: AsyncRead + Unpin> RecordReader<R> {
    /// Reads one complete record into `cipher` (prefix at 0..2, body after)
    /// and returns the body length. Progress survives cancellation.
    async fn read_record(&mut self) -> Result<usize, RecvError> {
        loop {
            let target = match self.body_len {
                None => LEN_PREFIX,
                Some(len) => LEN_PREFIX.saturating_add(len),
            };
            if self.filled == target {
                match self.body_len {
                    None => {
                        let mut prefix = [0u8; LEN_PREFIX];
                        prefix.copy_from_slice(self.cipher.get(..LEN_PREFIX).unwrap_or(&[0, 0]));
                        self.body_len = Some(parse_len(prefix, RecordPhase::Transport)?);
                        continue;
                    }
                    Some(len) => return Ok(len),
                }
            }
            let buf = self
                .cipher
                .get_mut(self.filled..target)
                .ok_or(RecvError::Protocol(ProtocolError::LimitExceeded("record")))?;
            match self.inner.read(buf).await {
                Ok(0) => return Err(RecvError::Transport(TransferError::ConnectionLost)),
                Ok(k) => self.filled = self.filled.saturating_add(k),
                Err(e) => return Err(RecvError::Transport(map_io(&e, Phase::Idle))),
            }
        }
    }
}

/// Encrypts and writes records. A write that makes no progress for the
/// stall timeout fails with `Timeout(Write)`.
pub struct RecordWriter<W> {
    inner: W,
    sealer: Sealer,
    plain: Box<[u8]>,
    cipher: Box<[u8]>,
    scratch: Vec<u8>,
    stall: Duration,
}

impl<W> fmt::Debug for RecordWriter<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecordWriter")
            .field("sealer", &self.sealer)
            .finish_non_exhaustive()
    }
}

impl<W: AsyncWrite + Unpin> RecordWriter<W> {
    pub(crate) fn new(inner: W, sealer: Sealer, stall: Duration) -> Self {
        Self {
            inner,
            sealer,
            plain: vec![0u8; MAX_PLAINTEXT].into_boxed_slice(),
            cipher: vec![0u8; MAX_RECORD].into_boxed_slice(),
            scratch: Vec::with_capacity(1024),
            stall,
        }
    }

    /// Sends one control message.
    ///
    /// # Errors
    /// Connection failures, write stalls, or an oversized message.
    pub async fn send(&mut self, msg: &Message) -> Result<(), TransferError> {
        self.scratch.clear();
        encode(msg, &mut self.scratch)?;
        let n = self.scratch.len();
        if n == 0 || n > MAX_FRAME_BODY {
            return Err(TransferError::Internal("control message too large".into()));
        }
        let (head, body) = self.plain.split_at_mut(1);
        head.copy_from_slice(&[1]);
        body.get_mut(..n)
            .ok_or_else(|| TransferError::Internal("buffer".into()))?
            .copy_from_slice(&self.scratch);
        self.seal_and_write(n.saturating_add(1)).await
    }

    /// Buffer for up to 65,518 bytes of file data; fill it, then call
    /// [`RecordWriter::send_data`].
    pub fn data_buf(&mut self) -> &mut [u8] {
        self.plain.get_mut(1..).unwrap_or_default()
    }

    /// Sends the first `n` bytes of [`RecordWriter::data_buf`] as a DATA frame.
    ///
    /// # Errors
    /// Connection failures, write stalls, or `n` outside 1..=65,518.
    pub async fn send_data(&mut self, n: usize) -> Result<(), TransferError> {
        if n == 0 || n > MAX_FRAME_BODY {
            return Err(TransferError::Internal("invalid data frame length".into()));
        }
        if let Some(b) = self.plain.first_mut() {
            *b = 2;
        }
        self.seal_and_write(n.saturating_add(1)).await
    }

    async fn seal_and_write(&mut self, plain_len: usize) -> Result<(), TransferError> {
        let plain = self
            .plain
            .get(..plain_len)
            .ok_or_else(|| TransferError::Internal("buffer".into()))?;
        let total = self.sealer.seal(plain, &mut self.cipher)?;
        let mut off = 0usize;
        while off < total {
            let chunk = self
                .cipher
                .get(off..total)
                .ok_or_else(|| TransferError::Internal("buffer".into()))?;
            match timeout(self.stall, self.inner.write(chunk)).await {
                Err(_) => return Err(TransferError::Timeout(Phase::Write)),
                Ok(Ok(0)) => return Err(TransferError::ConnectionLost),
                Ok(Ok(k)) => off = off.saturating_add(k),
                Ok(Err(e)) => return Err(map_io(&e, Phase::Write)),
            }
        }
        match timeout(self.stall, self.inner.flush()).await {
            Err(_) => Err(TransferError::Timeout(Phase::Write)),
            Ok(r) => r.map_err(|e| map_io(&e, Phase::Write)),
        }
    }

    /// Closes the write direction (best effort, bounded).
    pub async fn shutdown(&mut self) {
        let _ = timeout(Duration::from_secs(1), self.inner.shutdown()).await;
    }
}
