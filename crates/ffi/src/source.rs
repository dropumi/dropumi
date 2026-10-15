// SPDX-License-Identifier: MPL-2.0
//! Files to send: local paths, or documents the app reads for Rust.
//!
//! A path item is read by `LocalFileSource`, which refuses Dropumi's own
//! configuration files however they are named. A stream item (for example
//! an Android `content://` document) is read by the app: its metadata is
//! fetched when the send starts, on the caller's thread, and its bytes on a
//! reader thread of its own, so app code never runs on the runtime thread.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use dropumi_application::error::SourceError;
use dropumi_application::ports::{FileSource, SourceMeta};
use dropumi_domain::filename::wire_name_from_local;
use dropumi_infra::fs::LocalFileSource;
use tokio::io::{AsyncRead, ReadBuf};
use tokio::sync::mpsc;

use crate::error::CallbackError;

/// Bytes asked for per read.
const CHUNK: u32 = 256 * 1024;
/// Chunks buffered ahead of the transfer.
const READ_AHEAD: usize = 4;

/// A document the app reads on Rust's behalf.
#[uniffi::export(with_foreign)]
pub trait ForeignFile: Send + Sync {
    /// The name to offer (only its last component is used).
    fn display_name(&self) -> String;
    /// Exact size in bytes; documents of unknown size cannot be sent (copy
    /// them to a file first).
    fn size(&self) -> Option<u64>;
    /// Modification time (Unix seconds), if known.
    fn modified(&self) -> Option<i64>;
    /// Opens the document for reading. (Not named `open`: UniFFI's Swift
    /// generator escapes that keyword inside the C header too.)
    ///
    /// # Errors
    /// The document cannot be opened.
    fn open_reader(&self) -> Result<Arc<dyn ForeignReader>, CallbackError>;
}

/// An open document.
#[uniffi::export(with_foreign)]
pub trait ForeignReader: Send + Sync {
    /// Reads at most `max_len` bytes; an empty result means end of file.
    ///
    /// # Errors
    /// Reading failed.
    fn read(&self, max_len: u32) -> Result<Vec<u8>, CallbackError>;
    /// Closes the document. (Not named `close`: generated Kotlin objects
    /// already have `close()` from `AutoCloseable`.)
    fn close_document(&self);
}

/// Something to send.
#[derive(uniffi::Enum)]
pub enum SendItem {
    /// An absolute path the app may read.
    Path {
        /// The path.
        path: String,
    },
    /// A document the app reads.
    Stream {
        /// The document.
        file: Arc<dyn ForeignFile>,
    },
}

impl std::fmt::Debug for SendItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path { .. } => f.write_str("SendItem::Path"),
            Self::Stream { .. } => f.write_str("SendItem::Stream"),
        }
    }
}

struct StreamItem {
    file: Arc<dyn ForeignFile>,
    meta: SourceMeta,
    /// Set when the document did not have the size it announced.
    changed: Arc<AtomicBool>,
}

/// Path items and stream items behind one `FileSource`.
pub(crate) struct CompositeSource {
    local: LocalFileSource,
    streams: HashMap<PathBuf, StreamItem>,
}

impl std::fmt::Debug for CompositeSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeSource")
            .field("streams", &self.streams.len())
            .finish_non_exhaustive()
    }
}

impl CompositeSource {
    pub(crate) fn new(local: LocalFileSource) -> Self {
        Self {
            local,
            streams: HashMap::new(),
        }
    }

    /// Adds a stream item and returns the opaque key the use case passes
    /// back. Reads the document's metadata now, on the calling thread.
    pub(crate) fn add_stream(&mut self, file: Arc<dyn ForeignFile>) -> Result<PathBuf, String> {
        let size = file
            .size()
            .ok_or("a document of unknown size cannot be sent")?;
        let raw_name = file.display_name();
        let last = raw_name.rsplit(['/', '\\']).next().unwrap_or_default();
        let name = wire_name_from_local(OsStr::new(last));
        // Relative and never a valid absolute path, so it cannot collide
        // with a path item.
        let key = PathBuf::from(format!("ffi-item-{}", self.streams.len()));
        self.streams.insert(
            key.clone(),
            StreamItem {
                meta: SourceMeta {
                    name,
                    size,
                    modified: file.modified(),
                    // The app reports whole seconds only. A change of an
                    // app-read document is detected by its reader instead
                    // (the `changed` flag, set when the bytes read differ
                    // from the announced size), never by its times.
                    local_modified: None,
                },
                file,
                changed: Arc::new(AtomicBool::new(false)),
            },
        );
        Ok(key)
    }
}

/// Reads a stream item. Bytes come from a reader thread through a small
/// bounded channel; dropping the reader stops the thread after its current
/// read.
pub(crate) struct StreamReader {
    chunks: mpsc::Receiver<io::Result<Vec<u8>>>,
    pending: Vec<u8>,
    offset: usize,
}

/// The reader type of [`CompositeSource`].
pub(crate) enum SourceReader {
    Local(tokio::fs::File),
    Stream(StreamReader),
}

impl AsyncRead for SourceReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Local(f) => Pin::new(f).poll_read(cx, buf),
            Self::Stream(s) => s.poll_read(cx, buf),
        }
    }
}

impl StreamReader {
    fn poll_read(&mut self, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        loop {
            if let Some(rest) = self.pending.get(self.offset..)
                && !rest.is_empty()
            {
                let n = rest.len().min(buf.remaining());
                buf.put_slice(rest.get(..n).unwrap_or_default());
                self.offset = self.offset.saturating_add(n);
                return Poll::Ready(Ok(()));
            }
            match self.chunks.poll_recv(cx) {
                Poll::Pending => return Poll::Pending,
                // End of file.
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e)),
                Poll::Ready(Some(Ok(chunk))) => {
                    self.pending = chunk;
                    self.offset = 0;
                }
            }
        }
    }
}

fn spawn_reader(item: &StreamItem) -> Result<StreamReader, SourceError> {
    let (tx, rx) = mpsc::channel(READ_AHEAD);
    let file = Arc::clone(&item.file);
    let expected = item.meta.size;
    let changed = Arc::clone(&item.changed);
    std::thread::Builder::new()
        .name("dropumi-reader".into())
        .spawn(move || read_document(&*file, expected, &changed, &tx))
        .map_err(|e| SourceError::Unreadable {
            path: "shared document".into(),
            message: e.to_string(),
        })?;
    Ok(StreamReader {
        chunks: rx,
        pending: Vec::new(),
        offset: 0,
    })
}

/// The reader thread: opens the document, reads it in chunks and closes it.
fn read_document(
    file: &dyn ForeignFile,
    expected: u64,
    changed: &AtomicBool,
    tx: &mpsc::Sender<io::Result<Vec<u8>>>,
) {
    let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| file.open_reader()));
    let reader = match opened {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            let _ = tx.blocking_send(Err(io::Error::other(e.to_string())));
            return;
        }
        Err(_) => {
            let _ = tx.blocking_send(Err(io::Error::other("the app panicked opening a document")));
            return;
        }
    };
    let mut total: u64 = 0;
    loop {
        let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reader.read(CHUNK)));
        let chunk = match read {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                let _ = tx.blocking_send(Err(io::Error::other(e.to_string())));
                break;
            }
            Err(_) => {
                let _ =
                    tx.blocking_send(Err(io::Error::other("the app panicked reading a document")));
                break;
            }
        };
        if chunk.is_empty() {
            if total != expected {
                changed.store(true, Ordering::SeqCst);
            }
            break;
        }
        total = total.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        if total > expected {
            // Longer than announced: the document changed. The transfer
            // only reads the announced size, so stop here.
            changed.store(true, Ordering::SeqCst);
            break;
        }
        if tx.blocking_send(Ok(chunk)).is_err() {
            // The transfer stopped reading (finished or cancelled).
            break;
        }
    }
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reader.close_document()));
}

impl FileSource for CompositeSource {
    type Reader = SourceReader;

    async fn meta(&self, path: &Path) -> Result<SourceMeta, SourceError> {
        match self.streams.get(path) {
            Some(item) => Ok(item.meta.clone()),
            None => self.local.meta(path).await,
        }
    }

    async fn open(&self, path: &Path, meta: &SourceMeta) -> Result<SourceReader, SourceError> {
        match self.streams.get(path) {
            Some(item) => spawn_reader(item).map(SourceReader::Stream),
            None => self.local.open(path, meta).await.map(SourceReader::Local),
        }
    }

    async fn unchanged(&self, path: &Path, meta: &SourceMeta) -> bool {
        match self.streams.get(path) {
            Some(item) => !item.changed.load(Ordering::SeqCst),
            None => self.local.unchanged(path, meta).await,
        }
    }
}
