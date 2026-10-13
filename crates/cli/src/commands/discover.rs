// SPDX-License-Identifier: MPL-2.0
//! `dropumi discover`.

use std::sync::Arc;
use std::time::Duration;

use dropumi_application::usecases::discovery;

use super::{Ctx, event_printer};
use crate::args::DiscoverArgs;
use crate::exit::code;

/// Lists nearby devices.
pub async fn run(args: DiscoverArgs, ctx: &Ctx) -> u8 {
    let Some(disc) = super::discovery(args.mdns_loopback) else {
        return ctx.usage("this build has no mDNS discovery");
    };
    let (events, printer) = event_printer(Arc::clone(&ctx.out));
    let result = discovery::discover(&disc, Duration::from_millis(args.timeout_ms), &events).await;
    drop(events);
    let _ = printer.await;
    match result {
        Ok(peers) => {
            if peers.is_empty() {
                ctx.out.human("No devices found.");
            }
            code::OK
        }
        Err(e) => ctx.usage(&e.to_string()),
    }
}
