// SPDX-License-Identifier: MPL-2.0
//! Command implementations (the composition root).

pub mod discover;
pub mod identity;
pub mod pair;
pub mod receive;
pub mod send;
pub mod trust;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dropumi_application::config::Timeouts;
#[cfg(not(feature = "mdns"))]
use dropumi_application::error::DiscoveryError;
use dropumi_application::error::StoreError;
use dropumi_application::events::{AppEvent, EventSink};
use dropumi_application::identity::LocalIdentity;
use dropumi_application::ports::IdentityStore;
#[cfg(not(feature = "mdns"))]
use dropumi_application::ports::{AdHandle, Advertisement, Discovery};
#[cfg(not(feature = "mdns"))]
use dropumi_domain::discovery::DiscoveredPeer;
use dropumi_infra::paths;
use dropumi_infra::perms;
use dropumi_infra::store::{FileIdentityStore, TrustFile};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::args::{Cli, Command, TimeoutArgs};
use crate::control::{self, ControlActions, Prompts};
use crate::exit::code;
use crate::interaction::CliInteraction;
use crate::output::Output;

/// Shared state for one invocation.
#[derive(Debug)]
pub struct Ctx {
    /// Output.
    pub out: Arc<Output>,
    /// Configuration directory.
    pub config_dir: PathBuf,
    /// Whether `--control-stdin` was given.
    pub control_stdin: bool,
    /// Root cancellation (Ctrl-C, `quit`).
    pub root: CancellationToken,
}

impl Ctx {
    /// Opens the identity store, failing with exit code 9.
    ///
    /// # Errors
    /// The exit code on failure.
    pub fn identity(&self) -> Result<(FileIdentityStore, LocalIdentity), u8> {
        let store = FileIdentityStore::open(&self.config_dir).map_err(|e| self.store_error(&e))?;
        if let Some(w) = perms::config_dir_warning(&self.config_dir) {
            self.out.warning(&w);
        }
        let id = store.load_or_create().map_err(|e| self.store_error(&e))?;
        Ok((store, id))
    }

    /// Opens the trust store, failing with exit code 9.
    ///
    /// # Errors
    /// The exit code on failure.
    pub fn trust(&self) -> Result<TrustFile, u8> {
        TrustFile::open(&self.config_dir).map_err(|e| self.store_error(&e))
    }

    /// Reports a configuration error and returns exit code 9.
    #[must_use]
    pub fn store_error(&self, e: &StoreError) -> u8 {
        self.out.failed(
            None,
            &dropumi_domain::error::TransferError::Storage(e.to_string()),
        );
        code::LOCAL
    }

    /// Reports a configuration change that cannot be reported as applied
    /// and returns exit code 9.
    #[must_use]
    pub fn settle_error(&self, e: &SettleError) -> u8 {
        self.out.failed(
            None,
            &dropumi_domain::error::TransferError::Storage(e.message()),
        );
        code::LOCAL
    }

    /// Reports a usage error and returns exit code 2.
    #[must_use]
    pub fn usage(&self, message: &str) -> u8 {
        self.out.event(
            "failed",
            serde_json::json!({"code": "usage", "message": message}),
        );
        self.out.notice(&format!("Error: {message}"));
        code::USAGE
    }

    /// Starts the control channel (if requested) and builds the interaction.
    #[must_use]
    pub fn interaction(&self, cancel: Box<dyn Fn() + Send + Sync>) -> CliInteraction {
        let prompts: Option<Prompts> = self.control_stdin.then(|| {
            control::start(
                Arc::clone(&self.out),
                ControlActions {
                    cancel,
                    quit: self.root.clone(),
                },
            )
        });
        CliInteraction::new(prompts, Arc::clone(&self.out))
    }
}

/// The start of the failure message for a change that was saved but whose
/// new state could not be read back.
pub const SAVED_UNREADABLE: &str = "the change was saved, but it could not be read back";

/// Why a configuration change cannot be reported as applied.
#[derive(Debug)]
#[non_exhaustive]
pub enum SettleError {
    /// The write failed before its commit point: nothing changed.
    NotApplied(StoreError),
    /// The change was saved, but reading its new state back failed.
    Unreadable(StoreError),
}

impl SettleError {
    /// The local failure message (it may name local paths: for the
    /// command's own output only, never for logs).
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NotApplied(e) => e.to_string(),
            Self::Unreadable(e) => format!("{SAVED_UNREADABLE}: {e}"),
        }
    }
}

/// Settles the result of a configuration write for a command.
///
/// - `Ok(value)`: applied and durable; `read(Some(value))` gives the state
///   to print.
/// - [`StoreError::NotDurable`]: the change is in place, only the directory
///   sync failed. `warn` gets the warning (the error's local text) first,
///   then `read(None)` loads the applied state: the command prints the
///   warning and its normal output and exits 0.
/// - Any other write error: nothing was applied
///   ([`SettleError::NotApplied`], exit 9).
///
/// A failed `read` after either kind of saved write is
/// [`SettleError::Unreadable`]: its message says the change was saved. The
/// warning has gone out before the read, so it is never lost.
///
/// # Errors
/// The [`SettleError`] to report.
pub fn settle<W, T>(
    written: Result<W, StoreError>,
    warn: impl FnOnce(&str),
    read: impl FnOnce(Option<W>) -> Result<T, StoreError>,
) -> Result<T, SettleError> {
    let written = match written {
        Ok(value) => Some(value),
        Err(e @ StoreError::NotDurable(_)) => {
            warn(&e.to_string());
            None
        }
        Err(e) => return Err(SettleError::NotApplied(e)),
    };
    read(written).map_err(SettleError::Unreadable)
}

/// Applies hidden timeout overrides.
#[must_use]
pub fn timeouts(t: &TimeoutArgs) -> Timeouts {
    let mut out = Timeouts::default();
    let ms = Duration::from_millis;
    if let Some(v) = t.handshake_timeout_ms {
        out.handshake = ms(v);
    }
    if let Some(v) = t.idle_timeout_ms {
        out.idle = ms(v);
    }
    if let Some(v) = t.consent_timeout_ms {
        out.consent = ms(v);
        out.decision = ms(v.saturating_add(30_000));
        out.offer_unverified = ms(v.saturating_add(30_000));
    }
    if let Some(v) = t.pair_timeout_ms {
        out.sas = ms(v);
    }
    if let Some(v) = t.write_stall_timeout_ms {
        out.write_stall = ms(v);
    }
    out
}

/// Prints events until the channel closes.
#[must_use]
pub fn event_printer(out: Arc<Output>) -> (EventSink, JoinHandle<()>) {
    let (tx, mut rx) = mpsc::channel::<AppEvent>(256);
    let task = tokio::spawn(async move {
        while let Some(e) = rx.recv().await {
            out.app_event(&e);
        }
    });
    (EventSink::new(tx), task)
}

/// Discovery when the binary is built without mDNS.
#[cfg(not(feature = "mdns"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct NoDiscovery;

/// Advertisement handle of [`NoDiscovery`].
#[cfg(not(feature = "mdns"))]
#[derive(Debug)]
pub struct NoAd;

#[cfg(not(feature = "mdns"))]
impl AdHandle for NoAd {
    async fn stop(self) {}
    fn instance(&self) -> &str {
        ""
    }
}

#[cfg(not(feature = "mdns"))]
impl Discovery for NoDiscovery {
    type Handle = NoAd;

    async fn advertise(&self, _ad: Advertisement) -> Result<NoAd, DiscoveryError> {
        Err(DiscoveryError::Unavailable("built without mDNS".into()))
    }

    async fn browse(&self, _t: Duration) -> Result<Vec<DiscoveredPeer>, DiscoveryError> {
        Err(DiscoveryError::Unavailable("built without mDNS".into()))
    }
}

/// The discovery implementation compiled into this binary.
#[cfg(feature = "mdns")]
pub type Disc = dropumi_infra::mdns::MdnsDiscovery;
/// The discovery implementation compiled into this binary.
#[cfg(not(feature = "mdns"))]
pub type Disc = NoDiscovery;

/// Creates discovery, or `None` when built without mDNS.
#[must_use]
pub fn discovery(allow_loopback: bool) -> Option<Disc> {
    #[cfg(feature = "mdns")]
    {
        Some(dropumi_infra::mdns::MdnsDiscovery::new(allow_loopback))
    }
    #[cfg(not(feature = "mdns"))]
    {
        let _ = allow_loopback;
        None
    }
}

/// Runs the selected command and returns the exit code.
pub async fn dispatch(cli: Cli, root: CancellationToken) -> u8 {
    let out = Arc::new(Output::new(cli.json));
    let config_dir = match paths::config_dir(cli.config_dir.as_deref()) {
        Ok(d) => d,
        Err(e) => {
            out.notice(&format!("Error: {e}"));
            return code::LOCAL;
        }
    };
    let ctx = Ctx {
        out,
        config_dir,
        control_stdin: cli.control_stdin,
        root,
    };
    match cli.command {
        Command::Identity(c) => identity::run(c, &ctx),
        Command::Trust(c) => trust::run(c, &ctx),
        Command::Pair(a) => pair::run(a, &ctx).await,
        Command::Receive(a) => receive::run(a, &ctx).await,
        Command::Send(a) => send::run(a, &ctx).await,
        Command::Discover(a) => discover::run(a, &ctx).await,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    const PATH: &str = "/home/someone/.config/dropumi/device.json";

    fn not_durable() -> StoreError {
        StoreError::NotDurable(format!("{PATH}: I/O error"))
    }

    /// Regression: a write saved without a confirmed directory sync is
    /// applied. The warning goes out first, then the state is re-read and
    /// reported as a success (exit 0) instead of a failure.
    #[test]
    fn not_durable_write_warns_before_rereading() {
        let log = RefCell::new(Vec::new());
        let settled = settle(
            Err::<&str, _>(not_durable()),
            |w| log.borrow_mut().push(format!("warning: {w}")),
            |written| {
                log.borrow_mut().push(format!("re-read after {written:?}"));
                Ok("re-read")
            },
        );
        assert_eq!(settled.unwrap(), "re-read");
        let log = log.into_inner();
        assert_eq!(log.len(), 2, "{log:?}");
        assert!(
            log[0].starts_with("warning: the change was saved but could not be confirmed on disk"),
            "{log:?}"
        );
        assert_eq!(log[1], "re-read after None");
    }

    /// Regression: when the re-read after a not-durable write fails, the
    /// warning has still gone out, and the error says the change was
    /// saved (not a plain storage failure, which means nothing changed).
    #[test]
    fn failed_reread_after_not_durable_write_keeps_the_warning() {
        let log = RefCell::new(Vec::new());
        let settled = settle(
            Err::<&str, _>(not_durable()),
            |w| log.borrow_mut().push(format!("warning: {w}")),
            |_| -> Result<&str, StoreError> {
                log.borrow_mut().push("re-read".to_owned());
                Err(StoreError::Io(format!("{PATH}: permission denied")))
            },
        );
        let e = settled.unwrap_err();
        assert!(
            matches!(e, SettleError::Unreadable(StoreError::Io(_))),
            "{e:?}"
        );
        assert!(
            e.message()
                .starts_with("the change was saved, but it could not be read back: "),
            "{}",
            e.message()
        );
        let log = log.into_inner();
        assert_eq!(log.len(), 2, "{log:?}");
        assert!(log[0].starts_with("warning: "), "{log:?}");
        assert_eq!(log[1], "re-read");
    }

    /// A durable write needs no warning; the read gets the written value.
    #[test]
    fn durable_write_is_applied_without_a_warning() {
        let settled = settle(
            Ok("written"),
            |w| panic!("no warning after a durable write: {w}"),
            Ok,
        );
        assert_eq!(settled.unwrap(), Some("written"));
    }

    /// A read that fails after a durable write is also "saved, but not
    /// read back", never "nothing changed".
    #[test]
    fn failed_read_after_durable_write_is_saved_but_unreadable() {
        let settled = settle(
            Ok("written"),
            |w| panic!("no warning after a durable write: {w}"),
            |_| -> Result<&str, StoreError> {
                Err(StoreError::Corrupt {
                    path: PATH.into(),
                    reason: "bad JSON".into(),
                })
            },
        );
        let e = settled.unwrap_err();
        assert!(matches!(e, SettleError::Unreadable(_)), "{e:?}");
        assert!(
            e.message().starts_with("the change was saved"),
            "{}",
            e.message()
        );
    }

    /// Every other store error means nothing changed: a failure (exit 9),
    /// without a warning or a read.
    #[test]
    fn store_failure_is_a_failure() {
        for e in [
            StoreError::Io(format!("{PATH}: permission denied")),
            StoreError::InsecurePermissions {
                path: PATH.into(),
                hint: "chmod 600".into(),
            },
            StoreError::Corrupt {
                path: PATH.into(),
                reason: "bad JSON".into(),
            },
        ] {
            let text = e.to_string();
            let settled = settle(
                Err::<&str, _>(e),
                |w| panic!("no warning after a failed write: {w}"),
                |_| -> Result<&str, StoreError> { panic!("no read after a failed write") },
            );
            let e = settled.unwrap_err();
            assert!(matches!(e, SettleError::NotApplied(_)), "{e:?}");
            assert_eq!(e.message(), text);
        }
    }
}
