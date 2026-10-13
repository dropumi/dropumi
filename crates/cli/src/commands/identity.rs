// SPDX-License-Identifier: MPL-2.0
//! `dropumi identity`.

use dropumi_application::usecases::identity as uc;

use super::Ctx;
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
        IdentityCmd::SetName { name } => match uc::set_name(&store, &name) {
            Ok(id) => {
                ctx.out.identity(&id);
                code::OK
            }
            Err(e) => ctx.store_error(&e),
        },
        IdentityCmd::Reset { yes } => {
            if !yes {
                return ctx.usage(
                    "resetting replaces this device's key and every paired device must pair again; \
                     pass --yes to confirm",
                );
            }
            match uc::reset(&store) {
                Ok(id) => {
                    ctx.out.identity(&id);
                    code::OK
                }
                Err(e) => ctx.store_error(&e),
            }
        }
    }
}
