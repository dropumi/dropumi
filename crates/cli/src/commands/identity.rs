// SPDX-License-Identifier: MPL-2.0
//! `dropumi identity`.

use dropumi_application::error::StoreError;
use dropumi_application::identity::LocalIdentity;
use dropumi_application::usecases::identity as uc;
use dropumi_infra::store::FileIdentityStore;

use super::{Ctx, settle};
use crate::args::IdentityCmd;
use crate::exit::code;

/// Runs an identity subcommand.
pub fn run(cmd: IdentityCmd, ctx: &Ctx) -> u8 {
    let (store, id) = match ctx.identity() {
        Ok(v) => v,
        Err(c) => return c,
    };
    match cmd {
        IdentityCmd::Show => {
            ctx.out.identity(&id);
            code::OK
        }
        IdentityCmd::SetName { name } => changed(ctx, &store, uc::set_name(&store, &name)),
        IdentityCmd::Reset { yes } => {
            if !yes {
                return ctx.usage(
                    "resetting replaces this device's key and every paired device must pair again; \
                     pass --yes to confirm",
                );
            }
            changed(ctx, &store, uc::reset(&store))
        }
    }
}

/// Prints the identity after a change, or reports the failure. A change
/// saved without a confirmed directory sync prints the warning first, then
/// the identity re-read from disk.
fn changed(ctx: &Ctx, store: &FileIdentityStore, written: Result<LocalIdentity, StoreError>) -> u8 {
    let settled = settle(
        written,
        |w| ctx.out.warning(w),
        |id| match id {
            Some(id) => Ok(id),
            None => uc::show(store),
        },
    );
    match settled {
        Ok(id) => {
            ctx.out.identity(&id);
            code::OK
        }
        Err(e) => ctx.settle_error(&e),
    }
}
