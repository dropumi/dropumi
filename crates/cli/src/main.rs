// SPDX-License-Identifier: MPL-2.0
//! `dropumi`: send and receive files on the local network.
//!
//! Authenticated (Noise XX) and encrypted transfers between paired
//! devices, with no accounts, cloud or internet connection.

mod args;
mod commands;
mod control;
mod exit;
mod interaction;
mod output;

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Parser;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::prelude::*;

use crate::args::Cli;

fn init_logging(cli: &Cli) {
    let default = if cli.quiet {
        LevelFilter::ERROR
    } else {
        match cli.verbose {
            0 => LevelFilter::WARN,
            1 => LevelFilter::INFO,
            2 => LevelFilter::DEBUG,
            _ => LevelFilter::TRACE,
        }
    };
    let filter = std::env::var("DROPUMI_LOG")
        .ok()
        .and_then(|s| s.parse::<Targets>().ok())
        .unwrap_or_else(|| Targets::new().with_default(default));
    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(false)
                .with_filter(filter),
        )
        .try_init();
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(&cli);
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(8)
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(error = %e, "cannot start the async runtime");
            return exit::exit(exit::code::INTERNAL);
        }
    };
    let root = CancellationToken::new();
    let interrupted = Arc::new(AtomicBool::new(false));
    let code = runtime.block_on(async {
        let token = root.clone();
        let flag = Arc::clone(&interrupted);
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                flag.store(true, Ordering::SeqCst);
                token.cancel();
                // A second Ctrl-C does not wait for the graceful shutdown.
                // Leftover temp files are removed by the next `receive`.
                if tokio::signal::ctrl_c().await.is_ok() {
                    #[allow(clippy::exit, reason = "the user asked to stop now")]
                    std::process::exit(i32::from(exit::code::INTERRUPTED));
                }
            }
        });
        commands::dispatch(cli, root).await
    });
    // Background tasks (stdin reader, signal handler) must not keep the
    // process alive.
    runtime.shutdown_background();
    if interrupted.load(Ordering::SeqCst) {
        exit::exit(exit::code::INTERRUPTED)
    } else {
        exit::exit(code)
    }
}
