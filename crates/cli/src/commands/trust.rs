// SPDX-License-Identifier: MPL-2.0
//! `dropumi trust`.

use dropumi_application::usecases::trust as uc;
use dropumi_domain::device::DeviceName;
use dropumi_domain::identity::Fingerprint;
use dropumi_infra::clock::SystemClock;

use super::{Ctx, settle};
use crate::args::TrustCmd;
use crate::exit::code;

/// Runs a trust subcommand.
pub fn run(cmd: TrustCmd, ctx: &Ctx) -> u8 {
    let trust = match ctx.trust() {
        Ok(t) => t,
        Err(c) => return c,
    };
    match cmd {
        TrustCmd::List => match uc::list(&trust) {
            Ok(list) => {
                ctx.out.trusted(&list);
                code::OK
            }
            Err(e) => ctx.store_error(&e),
        },
        TrustCmd::Add { fingerprint, name } => {
            let Ok(fp) = Fingerprint::parse_full(&fingerprint) else {
                return ctx.usage("expected the full 52-character fingerprint (A-Z, 2-7)");
            };
            let added = uc::add_manual(
                &trust,
                &SystemClock,
                fp,
                name.as_deref().map(DeviceName::sanitize),
            )
            .and_then(|_| uc::list(&trust));
            match settle(added, || uc::list(&trust)) {
                Ok((list, warning)) => {
                    if let Some(w) = &warning {
                        ctx.out.warning(w);
                    }
                    ctx.out.trusted(&list);
                    code::OK
                }
                Err(e) => ctx.store_error(&e),
            }
        }
        TrustCmd::Remove { fingerprint } => {
            let Ok(fp) = Fingerprint::parse_full(&fingerprint) else {
                return ctx.usage("expected the full 52-character fingerprint (A-Z, 2-7)");
            };
            // After a removal saved without a confirmed directory sync the
            // removal itself is the applied state; whether the device was
            // trusted before is not known then (`existed` is null).
            match settle(uc::remove(&trust, &fp).map(Some), || Ok(None)) {
                Ok((existed, warning)) => {
                    if let Some(w) = &warning {
                        ctx.out.warning(w);
                    }
                    ctx.out.event(
                        "trust_removed",
                        serde_json::json!({"fingerprint": fp.to_full(), "existed": existed}),
                    );
                    ctx.out.human(match existed {
                        Some(true) => "Removed.",
                        Some(false) => "That device was not trusted.",
                        None => "That device is no longer trusted.",
                    });
                    code::OK
                }
                Err(e) => ctx.store_error(&e),
            }
        }
    }
}
