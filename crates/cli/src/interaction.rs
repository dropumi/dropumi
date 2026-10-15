// SPDX-License-Identifier: MPL-2.0
//! The interaction port for the command line.
//!
//! - `--control-stdin`: decisions come from control commands bound to the
//!   pending prompt (see [`crate::control`]);
//! - interactive terminal: prompts on stderr, answers from stdin. One
//!   long-lived thread reads the terminal (started at the first prompt), and
//!   each prompt discards lines typed before it, so an abandoned prompt
//!   leaves no blocked reader behind and a late answer never applies to a
//!   later prompt. Before every prompt, input still queued in the terminal
//!   is flushed (Unix only);
//! - neither: every offer is rejected and every code is denied (never an
//!   automatic yes), with a warning.

use std::io::{BufRead, IsTerminal};
use std::sync::Arc;

use dropumi_application::ports::{Decision, IncomingOffer, Interaction, SasPrompt, SasPurpose};
use dropumi_domain::transfer::RejectReason;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::control::Prompts;
use crate::output::{Output, peer_line};

enum Mode {
    Control(Prompts),
    /// Terminal lines; the reader starts at the first prompt so a receiver
    /// that never prompts never reads (or stops on) the terminal.
    Tty(Mutex<Option<UnboundedReceiver<String>>>),
    Never,
}

/// Command-line implementation of [`Interaction`].
pub struct CliInteraction {
    mode: Mode,
    out: Arc<Output>,
}

impl std::fmt::Debug for CliInteraction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mode = match self.mode {
            Mode::Control(_) => "control",
            Mode::Tty(_) => "tty",
            Mode::Never => "never",
        };
        f.debug_struct("CliInteraction")
            .field("mode", &mode)
            .finish()
    }
}

impl CliInteraction {
    /// Chooses the mode from the control channel and the terminal.
    #[must_use]
    pub fn new(control: Option<Prompts>, out: Arc<Output>) -> Self {
        let mode = match control {
            Some(prompts) => Mode::Control(prompts),
            None if std::io::stdin().is_terminal() => Mode::Tty(Mutex::new(None)),
            None => Mode::Never,
        };
        Self { mode, out }
    }

    async fn ask_tty(
        &self,
        lines: &Mutex<Option<UnboundedReceiver<String>>>,
        question: &str,
    ) -> bool {
        let mut lines = lines.lock().await;
        // Input still queued in the terminal (typed before the first prompt,
        // or a partial line typed since the last one) must not answer a
        // question the user has not seen yet. The reader thread only gets
        // complete lines, so flush the terminal before every prompt; `ask`
        // then drops complete lines the reader already took.
        discard_pending_input();
        let lines = lines.get_or_insert_with(terminal_lines);
        ask(lines, || self.out.notice(question)).await
    }
}

/// Discards terminal input that was typed but not read yet.
#[cfg(unix)]
fn discard_pending_input() {
    if let Err(e) =
        rustix::termios::tcflush(std::io::stdin(), rustix::termios::QueueSelector::IFlush)
    {
        tracing::debug!(error = %e, "could not discard pending terminal input");
    }
}

/// Not implemented on Windows: flushing the console input buffer needs an
/// unsafe system call (see docs/cli.md).
#[cfg(not(unix))]
fn discard_pending_input() {}

/// Reads terminal lines on one long-lived thread.
fn terminal_lines() -> UnboundedReceiver<String> {
    let (tx, rx) = unbounded_channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

/// Discards lines typed before the question, shows it, and reads one
/// answer. End of input is a "no".
async fn ask(lines: &mut UnboundedReceiver<String>, show: impl FnOnce()) -> bool {
    while lines.try_recv().is_ok() {}
    show();
    lines
        .recv()
        .await
        .is_some_and(|line| matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

impl Interaction for CliInteraction {
    async fn decide_offer(&self, offer: &IncomingOffer) -> Decision {
        let yes = match &self.mode {
            Mode::Control(prompts) => prompts.offer(offer.transfer_id.to_hex()).await,
            Mode::Tty(lines) => {
                let question = format!(
                    "Accept {} file(s) from {}? [y/N]",
                    offer.files.len(),
                    peer_line(&offer.peer, offer.sas.as_ref())
                );
                self.ask_tty(lines, &question).await
            }
            Mode::Never => {
                self.out
                    .warning("no terminal and no --control-stdin: rejecting the offer");
                false
            }
        };
        if yes {
            Decision::Accept
        } else {
            Decision::Reject(RejectReason::Declined)
        }
    }

    async fn confirm_sas(&self, prompt: &SasPrompt) -> bool {
        match &self.mode {
            Mode::Control(prompts) => prompts.sas().await,
            Mode::Tty(lines) => {
                let what = match prompt.purpose {
                    SasPurpose::Pairing => "Pair with",
                    SasPurpose::UnverifiedReceiver => "Send to",
                };
                let question = format!(
                    "{what} {}?\nDoes the other device show {}? [y/N]",
                    peer_line(&prompt.peer, None),
                    prompt.code
                );
                self.ask_tty(lines, &question).await
            }
            Mode::Never => {
                self.out
                    .warning("no terminal and no --control-stdin: verification code denied");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lines_typed_before_a_prompt_are_not_answers() {
        let (tx, mut rx) = unbounded_channel();
        // Typed for an abandoned prompt, or ahead of this one.
        tx.send("y".to_owned()).unwrap();
        let answer = ask(&mut rx, || tx.send("n".to_owned()).unwrap()).await;
        assert!(!answer);
        let answer = ask(&mut rx, || tx.send(" YES ".to_owned()).unwrap()).await;
        assert!(answer);
    }

    #[tokio::test]
    async fn end_of_input_is_no() {
        let (tx, mut rx) = unbounded_channel::<String>();
        drop(tx);
        assert!(!ask(&mut rx, || {}).await);
    }
}
