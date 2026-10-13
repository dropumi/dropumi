// SPDX-License-Identifier: MPL-2.0
//! Command-line arguments.

use std::net::IpAddr;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Free, open-source, local file sharing. Just drop it nearby.
#[derive(Debug, Parser)]
#[command(name = "dropumi", version, about, long_about = None)]
pub struct Cli {
    /// Configuration directory (identity key and trusted devices).
    #[arg(long, global = true, env = "DROPUMI_CONFIG_DIR", value_name = "DIR")]
    pub config_dir: Option<PathBuf>,

    /// Write machine-readable events (NDJSON) to stdout.
    #[arg(long, global = true)]
    pub json: bool,

    /// Read control commands (NDJSON) from stdin instead of prompting.
    #[arg(long, global = true)]
    pub control_stdin: bool,

    /// More log output on stderr (repeatable).
    #[arg(short, long, global = true, action = clap::ArgAction::Count, conflicts_with = "quiet")]
    pub verbose: u8,

    /// Only errors on stderr.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// The command.
    #[command(subcommand)]
    pub command: Command,
}

/// Commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show or change this device's identity.
    #[command(subcommand)]
    Identity(IdentityCmd),
    /// Manage trusted devices.
    #[command(subcommand)]
    Trust(TrustCmd),
    /// Pair with another device by comparing a 6-digit code.
    Pair(PairArgs),
    /// Receive files.
    Receive(ReceiveArgs),
    /// Send files.
    Send(SendArgs),
    /// List nearby devices (mDNS).
    Discover(DiscoverArgs),
}

/// `identity` subcommands.
#[derive(Debug, Subcommand)]
pub enum IdentityCmd {
    /// Show name and fingerprint (creates the identity on first use).
    Show,
    /// Change the device name.
    SetName {
        /// New name.
        name: String,
    },
    /// Replace the identity key; every device must pair again.
    Reset {
        /// Confirm the reset.
        #[arg(long)]
        yes: bool,
    },
}

/// `trust` subcommands.
#[derive(Debug, Subcommand)]
pub enum TrustCmd {
    /// List trusted devices.
    List,
    /// Trust a device by its full 52-character fingerprint.
    Add {
        /// Full fingerprint (compare it with the other device first).
        fingerprint: String,
        /// Local alias.
        #[arg(long)]
        name: Option<String>,
    },
    /// Stop trusting a device.
    Remove {
        /// Full fingerprint.
        fingerprint: String,
    },
}

/// Listening options shared by `receive` and `pair --listen`.
#[derive(Debug, Args, Clone)]
pub struct ListenArgs {
    /// Address to listen on.
    #[arg(long, default_value = "0.0.0.0")]
    pub bind: IpAddr,
    /// TCP port (0 picks a free port).
    #[arg(long)]
    pub port: Option<u16>,
    /// Do not advertise over mDNS.
    #[arg(long)]
    pub no_mdns: bool,
}

/// Hidden timeout overrides (milliseconds), for testing.
#[derive(Debug, Args, Clone, Default)]
pub struct TimeoutArgs {
    #[arg(long, hide = true)]
    pub handshake_timeout_ms: Option<u64>,
    #[arg(long, hide = true)]
    pub idle_timeout_ms: Option<u64>,
    #[arg(long, hide = true)]
    pub consent_timeout_ms: Option<u64>,
    #[arg(long, hide = true)]
    pub pair_timeout_ms: Option<u64>,
    #[arg(long, hide = true)]
    pub write_stall_timeout_ms: Option<u64>,
    /// Use loopback interfaces for mDNS (testing on one machine).
    #[arg(long, hide = true)]
    pub mdns_loopback: bool,
}

/// `pair` arguments.
#[derive(Debug, Args)]
pub struct PairArgs {
    /// Wait for another device to pair with this one.
    #[arg(long, conflicts_with = "to", required_unless_present = "to")]
    pub listen: bool,
    /// Device to pair with (address, fingerprint prefix or name).
    #[arg(long, value_name = "TARGET")]
    pub to: Option<String>,
    /// Require this fingerprint (at least 24 characters).
    #[arg(long, value_name = "FP", requires = "to")]
    pub expect_fingerprint: Option<String>,
    /// How long to wait for a pairing attempt.
    #[arg(long, default_value_t = 5)]
    pub window_minutes: u64,
    #[command(flatten)]
    pub listen_opts: ListenArgs,
    #[command(flatten)]
    pub timeouts: TimeoutArgs,
}

/// Who may send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PolicyArg {
    /// Only paired (trusted) devices.
    TrustedOnly,
    /// Any device, for a limited time; always asks, shows a code.
    Everyone,
}

/// What to do with an authorized offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AcceptArg {
    /// Always ask.
    Prompt,
    /// Accept trusted devices automatically; ask for others.
    Trusted,
    /// Reject everything.
    Never,
}

/// `receive` arguments.
#[derive(Debug, Args)]
pub struct ReceiveArgs {
    /// Directory for received files (must exist; default: Downloads).
    #[arg(long, env = "DROPUMI_RECEIVE_DIR", value_name = "DIR")]
    pub dir: Option<PathBuf>,
    /// Who may send.
    #[arg(long, value_enum, default_value_t = PolicyArg::TrustedOnly)]
    pub policy: PolicyArg,
    /// Duration of `--policy everyone`.
    #[arg(long, default_value_t = 10)]
    pub everyone_minutes: u64,
    /// What to do with offers.
    #[arg(long, value_enum, default_value_t = AcceptArg::Prompt)]
    pub accept: AcceptArg,
    /// Exit after the first transfer offer.
    #[arg(long)]
    pub once: bool,
    #[command(flatten)]
    pub listen_opts: ListenArgs,
    #[command(flatten)]
    pub timeouts: TimeoutArgs,
}

/// `send` arguments.
#[derive(Debug, Args)]
pub struct SendArgs {
    /// Files to send.
    #[arg(required = true, value_name = "FILE")]
    pub files: Vec<PathBuf>,
    /// Receiver: address, host:port, fingerprint prefix or device name.
    /// Repeat (addresses only) to try several addresses in order.
    #[arg(long, required = true, value_name = "TARGET")]
    pub to: Vec<String>,
    /// Require this fingerprint (at least 24 characters).
    #[arg(long, value_name = "FP")]
    pub expect_fingerprint: Option<String>,
    /// Allow an untrusted receiver after confirming a 6-digit code.
    #[arg(long)]
    pub allow_unverified: bool,
    /// Per-address connect timeout.
    #[arg(long, default_value_t = 3000)]
    pub connect_timeout_ms: u64,
    /// How long to look for name or fingerprint targets.
    #[arg(long, default_value_t = 3000)]
    pub discover_timeout_ms: u64,
    /// Limit bandwidth (bytes per second).
    #[arg(long, hide = true)]
    pub throttle_bps: Option<u64>,
    #[command(flatten)]
    pub timeouts: TimeoutArgs,
}

/// `discover` arguments.
#[derive(Debug, Args)]
pub struct DiscoverArgs {
    /// How long to listen.
    #[arg(long, default_value_t = 3000)]
    pub timeout_ms: u64,
    /// Use loopback interfaces (testing on one machine).
    #[arg(long, hide = true)]
    pub mdns_loopback: bool,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("dropumi").chain(args.iter().copied()))
    }

    #[test]
    fn args_parse_matrix() {
        Cli::command().debug_assert();
        let c = parse(&["--json", "send", "a.txt", "b.txt", "--to", "10.0.0.2:47321"]).unwrap();
        assert!(c.json);
        let Command::Send(s) = c.command else {
            panic!()
        };
        assert_eq!(s.files.len(), 2);
        assert_eq!(s.to, vec!["10.0.0.2:47321"]);
        let c = parse(&["send", "a", "--to", "1.2.3.4:1", "--to", "1.2.3.4:2"]).unwrap();
        let Command::Send(s) = c.command else {
            panic!()
        };
        assert_eq!(s.to.len(), 2);
        assert!(parse(&["send", "a"]).is_err(), "--to is required");
        assert!(parse(&["send", "--to", "x"]).is_err(), "files are required");
        let c = parse(&[
            "receive",
            "--port",
            "0",
            "--accept",
            "trusted",
            "--once",
            "--no-mdns",
        ])
        .unwrap();
        let Command::Receive(r) = c.command else {
            panic!()
        };
        assert_eq!(r.listen_opts.port, Some(0));
        assert_eq!(r.accept, AcceptArg::Trusted);
        assert!(parse(&["pair"]).is_err(), "--listen or --to");
        assert!(parse(&["pair", "--listen", "--to", "x"]).is_err());
        assert!(parse(&["identity", "reset"]).is_ok());
        assert!(parse(&["-v", "-q", "identity", "show"]).is_err());
        assert!(parse(&["trust", "add", "ABC", "--name", "Desk"]).is_ok());
        assert!(parse(&["receive", "--policy", "everyone", "--everyone-minutes", "3"]).is_ok());
        assert!(parse(&["send", "a", "--to", "x", "--throttle-bps", "100"]).is_ok());
    }
}
