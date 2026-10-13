// SPDX-License-Identifier: MPL-2.0
//! `--control-stdin`: NDJSON commands on stdin.
//!
//! `{"cmd":"accept"}`, `{"cmd":"reject"}`, `{"cmd":"confirm"}`,
//! `{"cmd":"deny"}`, `{"cmd":"cancel"}`, `{"cmd":"quit"}`. `accept` and
//! `reject` may name the offer with `"transfer_id"` (the hex id of the
//! `offer_received` event).
//!
//! A decision only answers a prompt that is waiting when the decision
//! arrives; decisions are never queued for a later prompt. Without a
//! `transfer_id`, an offer decision needs exactly one pending offer.
//! Anything else is dropped with a warning, as are unknown commands. End of
//! input is ignored (a closed pipe never stops a receiver).

use std::io::BufRead;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::output::Output;

/// A parsed control command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlCmd {
    /// Accept the pending offer.
    Accept,
    /// Reject the pending offer.
    Reject,
    /// Confirm the shown verification code.
    Confirm,
    /// Deny the shown verification code.
    Deny,
    /// Cancel active transfers.
    Cancel,
    /// Stop.
    Quit,
}

/// A command with its optional target offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    /// The command.
    pub cmd: ControlCmd,
    /// Hex transfer id the command is meant for, if given.
    pub transfer_id: Option<String>,
}

/// Parses one line.
#[must_use]
pub fn parse(line: &str) -> Option<Control> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    let cmd = match v.get("cmd")?.as_str()? {
        "accept" => ControlCmd::Accept,
        "reject" => ControlCmd::Reject,
        "confirm" => ControlCmd::Confirm,
        "deny" => ControlCmd::Deny,
        "cancel" => ControlCmd::Cancel,
        "quit" => ControlCmd::Quit,
        _ => return None,
    };
    let transfer_id = match v.get("transfer_id") {
        None | Some(Value::Null) => None,
        Some(id) => Some(id.as_str()?.to_owned()),
    };
    Some(Control { cmd, transfer_id })
}

/// What a pending prompt asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ask {
    /// An offer, by hex transfer id.
    Offer(String),
    /// A verification code.
    Sas,
}

#[derive(Debug)]
struct Pending {
    key: u64,
    ask: Ask,
    answer: oneshot::Sender<bool>,
}

#[derive(Debug, Default)]
struct Registry {
    next_key: u64,
    pending: Vec<Pending>,
}

/// Prompts currently waiting for a control decision.
#[derive(Debug, Clone, Default)]
pub struct Prompts(Arc<Mutex<Registry>>);

/// Unregisters its prompt when the wait ends or is abandoned.
#[derive(Debug)]
struct Registration {
    prompts: Prompts,
    key: u64,
}

impl Drop for Registration {
    fn drop(&mut self) {
        if let Ok(mut r) = self.prompts.0.lock() {
            r.pending.retain(|p| p.key != self.key);
        }
    }
}

impl Prompts {
    /// Waits for a decision on the offer `transfer_id` (`true` = accept).
    ///
    /// The prompt is registered when the future is first polled, before it
    /// yields. On the CLI's current-thread runtime that is before the event
    /// printer can show the offer, so an answer to the printed
    /// `offer_received` event always finds the prompt.
    pub async fn offer(&self, transfer_id: String) -> bool {
        self.wait(Ask::Offer(transfer_id)).await
    }

    /// Waits for a decision on a verification code (`true` = confirm).
    pub async fn sas(&self) -> bool {
        self.wait(Ask::Sas).await
    }

    async fn wait(&self, ask: Ask) -> bool {
        let Some((_registration, answer)) = self.register(ask) else {
            return false;
        };
        answer.await.unwrap_or(false)
    }

    fn register(&self, ask: Ask) -> Option<(Registration, oneshot::Receiver<bool>)> {
        let (tx, rx) = oneshot::channel();
        let mut r = self.0.lock().ok()?;
        let key = r.next_key;
        r.next_key = r.next_key.wrapping_add(1);
        r.pending.push(Pending {
            key,
            ask,
            answer: tx,
        });
        Some((
            Registration {
                prompts: self.clone(),
                key,
            },
            rx,
        ))
    }

    /// Answers the pending offer named by `transfer_id`, or the only
    /// pending offer. Returns why the decision was dropped.
    fn answer_offer(&self, transfer_id: Option<&str>, yes: bool) -> Result<(), &'static str> {
        self.answer(
            |ask| match (ask, transfer_id) {
                (Ask::Offer(id), Some(want)) => id.eq_ignore_ascii_case(want),
                (Ask::Offer(_), None) => true,
                (Ask::Sas, _) => false,
            },
            yes,
        )
        .map_err(|found| match (found, transfer_id) {
            (0, Some(_)) => "no pending offer has this transfer_id: decision ignored",
            (0, None) => "no offer is waiting for a decision: decision ignored",
            _ => "several offers are waiting: add \"transfer_id\" (decision ignored)",
        })
    }

    /// Answers the pending verification code.
    fn answer_sas(&self, yes: bool) -> Result<(), &'static str> {
        self.answer(|ask| *ask == Ask::Sas, yes)
            .map_err(|found| match found {
                0 => "no verification code is waiting for confirmation: command ignored",
                _ => "several verification codes are waiting: command ignored",
            })
    }

    /// Delivers `yes` to the single prompt matching `pred`; otherwise
    /// returns how many matched.
    fn answer(&self, pred: impl Fn(&Ask) -> bool, yes: bool) -> Result<(), usize> {
        let Ok(mut r) = self.0.lock() else {
            return Err(0);
        };
        let matching: Vec<usize> = r
            .pending
            .iter()
            .enumerate()
            .filter(|(_, p)| pred(&p.ask))
            .map(|(i, _)| i)
            .collect();
        let [i] = matching.as_slice() else {
            return Err(matching.len());
        };
        let p = r.pending.remove(*i);
        // A prompt abandoned at this very moment counts as absent.
        p.answer.send(yes).map_err(|_| 0)
    }
}

/// What `cancel` and `quit` do in the current command.
pub struct ControlActions {
    /// Cancels active transfers.
    pub cancel: Box<dyn Fn() + Send + Sync>,
    /// Stops the command.
    pub quit: CancellationToken,
}

impl std::fmt::Debug for ControlActions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ControlActions").finish_non_exhaustive()
    }
}

/// Starts reading stdin on a thread and routes commands.
#[must_use]
pub fn start(out: Arc<Output>, actions: ControlActions) -> Prompts {
    let (line_tx, mut line_rx) = mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            if line_tx.send(line).is_err() {
                break;
            }
        }
    });
    let prompts = Prompts::default();
    let router = prompts.clone();
    tokio::spawn(async move {
        while let Some(line) = line_rx.recv().await {
            if line.trim().is_empty() {
                continue;
            }
            let Some(c) = parse(&line) else {
                out.warning("unknown control command");
                continue;
            };
            let id = c.transfer_id.as_deref();
            let routed = match c.cmd {
                ControlCmd::Accept => router.answer_offer(id, true),
                ControlCmd::Reject => router.answer_offer(id, false),
                ControlCmd::Confirm => router.answer_sas(true),
                ControlCmd::Deny => router.answer_sas(false),
                ControlCmd::Cancel => {
                    (actions.cancel)();
                    Ok(())
                }
                ControlCmd::Quit => {
                    actions.quit.cancel();
                    Ok(())
                }
            };
            if let Err(why) = routed {
                out.warning(why);
            }
        }
    });
    prompts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        let accept = parse(r#"{"cmd":"accept"}"#).unwrap();
        assert_eq!(accept.cmd, ControlCmd::Accept);
        assert_eq!(accept.transfer_id, None);
        assert_eq!(
            parse(r#" {"cmd":"reject","transfer_id":"ab12"} "#),
            Some(Control {
                cmd: ControlCmd::Reject,
                transfer_id: Some("ab12".into())
            })
        );
        assert_eq!(
            parse(r#"{"cmd":"quit","transfer_id":null}"#).map(|c| c.cmd),
            Some(ControlCmd::Quit)
        );
        assert_eq!(parse(r#"{"cmd":"accept","transfer_id":7}"#), None);
        assert_eq!(parse(r#"{"cmd":"explode"}"#), None);
        assert_eq!(parse("not json"), None);
    }

    #[test]
    fn decision_without_a_prompt_is_never_queued() {
        let prompts = Prompts::default();
        assert!(prompts.answer_offer(None, true).is_err());
        assert!(prompts.answer_offer(Some("aa"), true).is_err());
        let (_reg, mut rx) = prompts.register(Ask::Offer("aa".into())).unwrap();
        assert!(rx.try_recv().is_err(), "the earlier accept must not apply");
    }

    #[test]
    fn transfer_id_binds_the_decision() {
        let prompts = Prompts::default();
        let (_a, mut rx_a) = prompts.register(Ask::Offer("aa".into())).unwrap();
        let (_b, mut rx_b) = prompts.register(Ask::Offer("bb".into())).unwrap();
        assert!(prompts.answer_offer(None, true).is_err(), "ambiguous");
        assert!(prompts.answer_offer(Some("cc"), true).is_err());
        assert!(prompts.answer_offer(Some("BB"), false).is_ok());
        assert_eq!(rx_b.try_recv(), Ok(false));
        assert!(rx_a.try_recv().is_err());
        assert!(prompts.answer_offer(None, true).is_ok(), "one left");
        assert_eq!(rx_a.try_recv(), Ok(true));
    }

    #[test]
    fn abandoned_prompt_takes_no_decision() {
        let prompts = Prompts::default();
        let (reg, rx) = prompts.register(Ask::Offer("aa".into())).unwrap();
        drop(reg);
        drop(rx);
        assert!(prompts.answer_offer(Some("aa"), true).is_err());
        assert!(prompts.answer_offer(None, true).is_err());
    }

    #[test]
    fn offer_and_sas_decisions_do_not_mix() {
        let prompts = Prompts::default();
        let (_s, mut rx) = prompts.register(Ask::Sas).unwrap();
        assert!(prompts.answer_offer(None, true).is_err());
        assert!(prompts.answer_sas(true).is_ok());
        assert_eq!(rx.try_recv(), Ok(true));
        assert!(prompts.answer_sas(true).is_err(), "answered once");
    }
}
