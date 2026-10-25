// SPDX-License-Identifier: MPL-2.0
//! `dropumi receive`.

use std::sync::Arc;
use std::time::Duration;

use dropumi_application::config::{AdmissionLimits, ReceiveConfig};
use dropumi_application::ports::{AdHandle, Clock};
use dropumi_application::usecases::discovery::advertise;
use dropumi_application::usecases::receive::{
    ReceiveDeps, ReceiveFiles, ReceiveSummary, TransferCanceller,
};
use dropumi_domain::policy::{AcceptMode, PolicyMode, TransferPolicy};
use dropumi_infra::clock::SystemClock;
use dropumi_infra::fs::ReceiveDir;
use dropumi_infra::fs::cleanup::{STALE_AFTER, remove_stale_parts};
use dropumi_infra::net::{DEFAULT_PORT, TcpListenerAdapter};
use dropumi_infra::paths;

use super::{Ctx, event_printer, timeouts};
use crate::args::{AcceptArg, PolicyArg, ReceiveArgs};
use crate::exit::{self, code};

/// Receives files until stopped (or after one offer with `--once`).
pub async fn run(args: ReceiveArgs, ctx: &Ctx) -> u8 {
    let (_store, id) = match ctx.identity() {
        Ok(v) => v,
        Err(c) => return c,
    };
    let trust = match ctx.trust() {
        Ok(t) => t,
        Err(c) => return c,
    };
    ctx.out.set_local(&id);
    let Some(dir) = args.dir.clone().or_else(paths::default_receive_dir) else {
        ctx.out
            .notice("Error: no receive directory; pass --dir or set DROPUMI_RECEIVE_DIR");
        return code::LOCAL;
    };
    let sink = match ReceiveDir::open(&dir) {
        Ok(s) => s,
        Err(e) => {
            ctx.out.notice(&format!("Error: {e}"));
            return code::LOCAL;
        }
    };
    let removed = remove_stale_parts(sink.path(), STALE_AFTER);
    if removed > 0 {
        ctx.out
            .warning(&format!("removed {removed} stale partial file(s)"));
    }
    let explicit_port = args.listen_opts.port.is_some();
    let listener = match TcpListenerAdapter::bind(
        args.listen_opts.bind,
        args.listen_opts.port.unwrap_or(DEFAULT_PORT),
        explicit_port,
    )
    .await
    {
        Ok(l) => l,
        Err(e) => {
            ctx.out.notice(&format!("Error: {e}"));
            return code::NETWORK;
        }
    };
    let port = listener.port();
    let clock = SystemClock;
    let policy = TransferPolicy {
        mode: match args.policy {
            PolicyArg::TrustedOnly => PolicyMode::TrustedOnly,
            PolicyArg::Everyone => PolicyMode::Everyone {
                until: clock.now().saturating_add(Duration::from_secs(
                    args.everyone_minutes.saturating_mul(60),
                )),
            },
        },
        accept: match args.accept {
            AcceptArg::Prompt => AcceptMode::Prompt,
            AcceptArg::Trusted => AcceptMode::AutoTrusted,
            AcceptArg::Never => AcceptMode::Never,
        },
    };
    let canceller = TransferCanceller::default();
    let on_cancel = canceller.clone();
    let interaction = ctx.interaction(Box::new(move || on_cancel.cancel_active()));
    let (events, printer) = event_printer(Arc::clone(&ctx.out));

    let disc = if args.listen_opts.no_mdns {
        None
    } else {
        super::discovery(args.timeouts.mdns_loopback)
    };
    let ad = match &disc {
        Some(d) => advertise(d, &id, port, &events).await,
        None => None,
    };

    let receiver = ReceiveFiles::new(
        listener,
        ReceiveDeps {
            sink,
            trust,
            interaction,
            clock,
            identity: id,
            timeouts: timeouts(&args.timeouts),
            limits: AdmissionLimits::default(),
            events,
        },
    )
    .with_canceller(canceller);
    let summary = receiver
        .run(
            ReceiveConfig {
                policy,
                once: args.once,
            },
            ctx.root.clone(),
        )
        .await;
    if let Some(ad) = ad {
        ad.stop().await;
    }
    let _ = printer.await;
    match summary {
        Ok(ReceiveSummary::Once(outcome)) => exit::for_outcome(&outcome),
        Ok(ReceiveSummary::Stopped) => code::OK,
        Err(e) => {
            ctx.out.notice(&format!("Error: {e}"));
            exit::for_usecase(&e)
        }
    }
}
