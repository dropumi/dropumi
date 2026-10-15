// SPDX-License-Identifier: MPL-2.0
//! The node's tokio runtime and the threads that call app code.
//!
//! One `current_thread` runtime runs on its own thread (`dropumi-rt`), the
//! same model as the CLI. App code is never called on that thread or while
//! a Rust lock is held: every call into Swift or Kotlin made from Rust runs
//! on a short-lived thread of its own (`dropumi-app`), so a slow or stuck
//! app callback can never stall a transfer.

use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::Duration;

use tokio::runtime::{Builder, Handle};
use tokio::sync::oneshot;

use crate::error::DropumiError;

/// Blocking threads for file I/O (`tokio::fs`).
const MAX_BLOCKING_THREADS: usize = 8;

/// A runtime on a dedicated thread.
#[derive(Debug)]
pub(crate) struct Runtime {
    handle: Handle,
    stop: Mutex<Option<oneshot::Sender<()>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Runtime {
    /// Starts the runtime thread.
    pub(crate) fn start() -> Result<Self, DropumiError> {
        let rt = Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(MAX_BLOCKING_THREADS)
            .thread_name("dropumi-blocking")
            .build()
            .map_err(|e| DropumiError::internal(format!("cannot start the runtime: {e}")))?;
        let handle = rt.handle().clone();
        let (stop, stopped) = oneshot::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("dropumi-rt".into())
            .spawn(move || {
                rt.block_on(async {
                    let _ = stopped.await;
                });
                rt.shutdown_timeout(Duration::from_secs(2));
            })
            .map_err(|e| DropumiError::internal(format!("cannot start the runtime thread: {e}")))?;
        Ok(Self {
            handle,
            stop: Mutex::new(Some(stop)),
            thread: Mutex::new(Some(thread)),
        })
    }

    pub(crate) fn handle(&self) -> &Handle {
        &self.handle
    }

    /// Runs `fut` on the runtime and waits for it on the calling thread.
    ///
    /// Exported calls come from app threads, never from the runtime thread;
    /// a call from inside the runtime is refused instead of deadlocking.
    pub(crate) fn block_on<F>(&self, fut: F) -> Result<F::Output, DropumiError>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        if Handle::try_current().is_ok() {
            return Err(DropumiError::internal(
                "a blocking node call was made from inside the node's runtime",
            ));
        }
        let task = self.handle.spawn(fut);
        self.handle
            .block_on(task)
            .map_err(|e| DropumiError::internal(format!("node task failed: {e}")))
    }

    /// Stops the runtime: running tasks are dropped after a short grace
    /// period. With `wait`, also waits (bounded) for the thread to end.
    pub(crate) fn stop(&self, wait: bool) {
        if let Some(stop) = lock(&self.stop).take() {
            let _ = stop.send(());
        }
        let Some(thread) = lock(&self.thread).take() else {
            return;
        };
        if !wait {
            return;
        }
        let (done, ended) = std::sync::mpsc::channel();
        let joiner = std::thread::Builder::new()
            .name("dropumi-join".into())
            .spawn(move || {
                let _ = thread.join();
                let _ = done.send(());
            });
        if joiner.is_ok() {
            let _ = ended.recv_timeout(Duration::from_secs(5));
        }
    }
}

/// Runs app code on its own thread and returns its result asynchronously.
/// A panic in `f` becomes an error.
pub(crate) fn on_app_thread<T, F>(f: F) -> oneshot::Receiver<Result<T, DropumiError>>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    let spawned = std::thread::Builder::new()
        .name("dropumi-app".into())
        .spawn(move || {
            let out = catch_unwind(AssertUnwindSafe(f))
                .map_err(|_| DropumiError::internal("app callback panicked"));
            let _ = tx.send(out);
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "cannot start a thread for an app callback");
    }
    rx
}

/// Locks, recovering from a poisoned mutex (a panic elsewhere must not
/// take the node down with it).
pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
