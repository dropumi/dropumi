// SPDX-License-Identifier: MPL-2.0
//! `dropumi trust`.

use dropumi_application::usecases::trust as uc;
use dropumi_domain::device::DeviceName;
use dropumi_domain::identity::Fingerprint;
use dropumi_infra::clock::SystemClock;

use super::Ctx;
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
            match uc::add_manual(
                &trust,
                &SystemClock,
                fp,
                name.as_deref().map(DeviceName::sanitize),
            ) {
                Ok(_) => match uc::list(&trust) {
                    Ok(list) => {
                        ctx.out.trusted(&list);
                        code::OK
                    }
                    Err(e) => ctx.store_error(&e),
                },
                Err(e) => ctx.store_error(&e),
            }
        }
        TrustCmd::Remove { fingerprint } => {
            let Ok(fp) = Fingerprint::parse_full(&fingerprint) else {
                return ctx.usage("expected the full 52-character fingerprint (A-Z, 2-7)");
            };
            match uc::remove(&trust, &fp) {
                Ok(existed) => {
                    ctx.out.event(
                        "trust_removed",
                        serde_json::json!({"fingerprint": fp.to_full(), "existed": existed}),
                    );
                    ctx.out.human(if existed {
                        "Removed."
                    } else {
                        "That device was not trusted."
                    });
                    code::OK
                }
                Err(e) => ctx.store_error(&e),
            }
        }
    }
}
