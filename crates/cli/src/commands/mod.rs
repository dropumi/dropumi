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
