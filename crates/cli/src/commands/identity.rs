// SPDX-License-Identifier: MPL-2.0
//! `dropumi identity`.

use dropumi_application::error::StoreError;
use dropumi_application::identity::LocalIdentity;
use dropumi_application::usecases::identity as uc;

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
        IdentityCmd::SetName { name } => settled_identity(
            ctx,
            settle(uc::set_name(&store, &name), || uc::show(&store)),
        ),
        IdentityCmd::Reset { yes } => {
            if !yes {
                return ctx.usage(
                    "resetting replaces this device's key and every paired device must pair again; \
                     pass --yes to confirm",
                );
            }
            settled_identity(ctx, settle(uc::reset(&store), || uc::show(&store)))
        }
    }
}

/// Prints the identity after a change (with the warning if the change was
/// saved without a confirmed directory sync), or reports the failure.
fn settled_identity(ctx: &Ctx, settled: Result<(LocalIdentity, Option<String>), StoreError>) -> u8 {
    match settled {
        Ok((id, warning)) => {
            if let Some(w) = &warning {
                ctx.out.warning(w);
            }
            ctx.out.identity(&id);
            code::OK
        }
        Err(e) => ctx.store_error(&e),
    }
}
