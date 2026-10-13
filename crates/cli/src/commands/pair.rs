// SPDX-License-Identifier: MPL-2.0
//! `dropumi pair`.

use std::sync::Arc;
use std::time::Duration;

use dropumi_application::events::AppEvent;
use dropumi_application::ports::{AdHandle, Listener};
use dropumi_application::usecases::connect::ResolveParams;
use dropumi_application::usecases::discovery::advertise;
use dropumi_application::usecases::pair::{PairDeps, PairOutcome, pair_listen, pair_with};
use dropumi_domain::discovery::Target;
use dropumi_domain::identity::FingerprintPrefix;
use dropumi_domain::policy::TransferPolicy;
use dropumi_infra::clock::SystemClock;
use dropumi_infra::net::{DEFAULT_PORT, TcpConnector, TcpListenerAdapter};

use super::{Ctx, event_printer, timeouts};
use crate::args::PairArgs;
use crate::exit::{self, code};

/// Pairs with another device.
pub async fn run(args: PairArgs, ctx: &Ctx) -> u8 {
    let (_store, id) = match ctx.identity() {
        Ok(v) => v,
        Err(c) => return c,
    };
    let trust = match ctx.trust() {
        Ok(t) => t,
        Err(c) => return c,
    };
    ctx.out.set_local(&id);
    let root = ctx.root.clone();
    let interaction = ctx.interaction(Box::new(move || root.cancel()));
    let (events, printer) = event_printer(Arc::clone(&ctx.out));
    let disc = if args.listen_opts.no_mdns {
        None
    } else {
        super::discovery(args.timeouts.mdns_loopback)
    };
    let deps = PairDeps {
        trust: &trust,
        interaction: &interaction,
        clock: &SystemClock,
        identity: &id,
        timeouts: timeouts(&args.timeouts),
        events: events.clone(),
    };
    let result = if args.listen {
        let listener = match TcpListenerAdapter::bind(
            args.listen_opts.bind,
            args.listen_opts.port.unwrap_or(DEFAULT_PORT),
            args.listen_opts.port.is_some(),
        )
        .await
        {
            Ok(l) => l,
            Err(e) => {
                ctx.out.notice(&format!("Error: {e}"));
                return code::NETWORK;
            }
        };
        events
            .emit(AppEvent::Listening {
                addrs: listener.local_addrs(),
                policy: TransferPolicy::default(),
            })
            .await;
        let ad = match &disc {
            Some(d) => advertise(d, &id, listener.port(), &events).await,
            None => None,
        };
        let window = Duration::from_secs(args.window_minutes.saturating_mul(60));
        let r = pair_listen(listener, &deps, window, ctx.root.clone()).await;
        if let Some(ad) = ad {
            ad.stop().await;
        }
        r
    } else {
        let raw = args.to.unwrap_or_default();
        let target = match Target::parse(&raw) {
            Ok(t) => t,
            Err(e) => return ctx.usage(&format!("invalid target {raw:?}: {e}")),
        };
        if target.needs_discovery() && disc.is_none() {
            return ctx.usage(if args.listen_opts.no_mdns {
                "--no-mdns disables discovery, so name and fingerprint targets cannot be found: use an address (IP:port)"
            } else {
                "this build has no mDNS discovery: use an address (IP:port)"
            });
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
        let targets = [target];
        pair_with(
            &TcpConnector,
            disc.as_ref(),
            &deps,
            ResolveParams {
                targets: &targets,
                explicit: expect,
                discover_timeout: Duration::from_secs(3),
                allow_loopback: args.timeouts.mdns_loopback,
            },
            ctx.root.clone(),
        )
        .await
    };
    drop(deps);
    drop(events);
    let _ = printer.await;
    match result {
        Ok(PairOutcome::Paired(_)) => code::OK,
        Ok(PairOutcome::Cancelled) => {
            ctx.out.pairing_cancelled();
            code::CANCELLED
        }
        Ok(PairOutcome::Failed(e)) => {
            ctx.out.failed(None, &e);
            exit::for_error(&e)
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
