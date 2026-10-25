// SPDX-License-Identifier: MPL-2.0
//! `dropumi send`.

use std::sync::Arc;
use std::time::Duration;

use dropumi_application::config::SendRequest;
use dropumi_application::usecases::send::{SendDeps, SendFiles};
use dropumi_domain::discovery::Target;
use dropumi_domain::identity::FingerprintPrefix;
use dropumi_infra::clock::SystemClock;
use dropumi_infra::fs::LocalFileSource;
use dropumi_infra::net::TcpConnector;

use super::{Ctx, event_printer, timeouts};
use crate::args::SendArgs;
use crate::exit::{self, code};

/// Maximum number of `--to` addresses.
pub const MAX_TARGETS: usize = 8;

/// Sends files.
pub async fn run(args: SendArgs, ctx: &Ctx) -> u8 {
    let (_store, id) = match ctx.identity() {
        Ok(v) => v,
        Err(c) => return c,
    };
    let trust = match ctx.trust() {
        Ok(t) => t,
        Err(c) => return c,
    };
    ctx.out.set_local(&id);
    if args.to.len() > MAX_TARGETS {
        return ctx.usage("at most 8 --to addresses");
    }
    let mut targets = Vec::with_capacity(args.to.len());
    for raw in &args.to {
        match Target::parse(raw) {
            Ok(t) => targets.push(t),
            Err(e) => return ctx.usage(&format!("invalid target {raw:?}: {e}")),
        }
    }
    if targets.len() > 1 && targets.iter().any(Target::needs_discovery) {
        return ctx.usage("repeat --to only with addresses");
    }
    if targets.iter().any(Target::needs_discovery) && super::discovery(false).is_none() {
        return ctx.usage("this build has no mDNS discovery: use an address (IP:port)");
    }
    let expect = match args
        .expect_fingerprint
        .as_deref()
        .map(FingerprintPrefix::parse_pin)
        .transpose()
    {
        Ok(e) => e,
        Err(e) => return ctx.usage(&format!("--expect-fingerprint: {e}")),
    };
    let cancel = ctx.root.child_token();
    let on_cancel = cancel.clone();
    let interaction = ctx.interaction(Box::new(move || on_cancel.cancel()));
    let disc = super::discovery(args.timeouts.mdns_loopback);
    let mut t = timeouts(&args.timeouts);
    t.connect = Duration::from_millis(args.connect_timeout_ms);
    let req = SendRequest {
        targets,
        paths: args.files,
        expect,
        allow_unverified: args.allow_unverified,
        throttle_bps: args.throttle_bps,
        discover_timeout: Duration::from_millis(args.discover_timeout_ms),
        allow_loopback: args.timeouts.mdns_loopback,
    };
    let (events, printer) = event_printer(Arc::clone(&ctx.out));
    let source = LocalFileSource::default();
    let result = {
        let use_case = SendFiles::new(SendDeps {
            connector: &TcpConnector,
            discovery: disc.as_ref(),
            source: &source,
            trust: &trust,
            interaction: &interaction,
            clock: &SystemClock,
            identity: &id,
            timeouts: t,
            events,
        });
        use_case.run(req, cancel).await
    };
    let _ = printer.await;
    match result {
        Ok(outcome) => {
            ctx.out.outcome(&outcome);
            exit::for_outcome(&outcome)
        }
        Err(e) => {
            let c = exit::for_usecase(&e);
            if c == code::USAGE {
                ctx.usage(&e.to_string())
            } else {
                ctx.out.notice(&format!("Error: {e}"));
                c
            }
        }
    }
}
