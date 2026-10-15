// SPDX-License-Identifier: MPL-2.0
//! Questions for the local user, answered by prompt id.
//!
//! Ported from the CLI's control registry (`crates/cli/src/control.rs`),
//! with one change: a prompt is registered *before* `PromptOpened` is
//! published, so an answer to a published prompt always finds it, whatever
//! thread the app answers from.
//!
//! - Ids are never reused.
//! - An answer is delivered only to the prompt it names, and only while the
//!   prompt waits; answers are never queued for a later prompt.
//! - When the core stops waiting (timeout, cancel, session end), the prompt
//!   is removed and `PromptClosed { Withdrawn }` is published.
//! - Nothing here answers on its own. Automatic acceptance exists only in
//!   the core (`AutoTrusted`, verified peers only).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dropumi_application::config::Timeouts;
use dropumi_application::ports::{Decision, IncomingOffer, Interaction, SasPrompt, SasPurpose};
use dropumi_domain::filename::SafeFileName;
use dropumi_domain::transfer::RejectReason;
use tokio::sync::oneshot;

use crate::error::DropumiError;
use crate::mailbox::Mailbox;
use crate::runtime::lock;
use crate::types::{CodePurpose, NodeEvent, OfferedFile, PeerInfo, Prompt, PromptCloseReason};

/// Files listed inline in an offer prompt; the rest via `offer_files`.
pub(crate) const INLINE_FILES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnswerKind {
    Offer,
    Code,
}

#[derive(Debug)]
struct Pending {
    prompt: Prompt,
    files: Vec<OfferedFile>,
    answer: oneshot::Sender<bool>,
}

/// Prompts waiting for an answer.
#[derive(Debug)]
pub(crate) struct PromptRegistry {
    pending: Mutex<HashMap<u64, Pending>>,
    next_id: AtomicU64,
    mailbox: Arc<Mailbox>,
    timeouts: Timeouts,
}

/// Removes its prompt (and publishes `PromptClosed`) when dropped.
#[derive(Debug)]
struct Registration {
    registry: Arc<PromptRegistry>,
    id: u64,
}

impl Drop for Registration {
    fn drop(&mut self) {
        let still_waiting = lock(&self.registry.pending).remove(&self.id).is_some();
        self.registry.mailbox.push(NodeEvent::PromptClosed {
            id: self.id,
            reason: if still_waiting {
                PromptCloseReason::Withdrawn
            } else {
                PromptCloseReason::Answered
            },
        });
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn expires_at(after: Duration) -> u64 {
    now_ms().saturating_add(u64::try_from(after.as_millis()).unwrap_or(u64::MAX))
}

impl PromptRegistry {
    pub(crate) fn new(mailbox: Arc<Mailbox>, timeouts: Timeouts) -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::default(),
            next_id: AtomicU64::new(1),
            mailbox,
            timeouts,
        })
    }

    /// Registers a prompt, then publishes it.
    fn register(
        self: &Arc<Self>,
        build: impl FnOnce(u64) -> Prompt,
        files: Vec<OfferedFile>,
    ) -> (Registration, oneshot::Receiver<bool>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let prompt = build(id);
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(
            id,
            Pending {
                prompt: prompt.clone(),
                files,
                answer: tx,
            },
        );
        self.mailbox.push(NodeEvent::PromptOpened { prompt });
        (
            Registration {
                registry: Arc::clone(self),
                id,
            },
            rx,
        )
    }

    /// Delivers an answer to the waiting prompt `id`.
    pub(crate) fn answer(&self, id: u64, kind: AnswerKind, yes: bool) -> Result<(), DropumiError> {
        let mut pending = lock(&self.pending);
        let Some(p) = pending.get(&id) else {
            return Err(DropumiError::PromptNotFound);
        };
        let matches = matches!(
            (kind, &p.prompt),
            (AnswerKind::Offer, Prompt::Offer { .. }) | (AnswerKind::Code, Prompt::Code { .. })
        );
        if !matches {
            return Err(DropumiError::PromptKindMismatch);
        }
        let Some(p) = pending.remove(&id) else {
            return Err(DropumiError::PromptNotFound);
        };
        drop(pending);
        // A prompt abandoned at this very moment counts as gone.
        p.answer.send(yes).map_err(|_| DropumiError::PromptNotFound)
    }

    /// Prompts waiting now, oldest first.
    pub(crate) fn waiting(&self) -> Vec<Prompt> {
        let mut prompts: Vec<Prompt> = lock(&self.pending)
            .values()
            .map(|p| p.prompt.clone())
            .collect();
        prompts.sort_by_key(Prompt::id);
        prompts
    }

    /// A page of an offer's files.
    pub(crate) fn offer_files(
        &self,
        id: u64,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<OfferedFile>, DropumiError> {
        let pending = lock(&self.pending);
        let p = pending.get(&id).ok_or(DropumiError::PromptNotFound)?;
        if !matches!(p.prompt, Prompt::Offer { .. }) {
            return Err(DropumiError::PromptKindMismatch);
        }
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        let len = usize::try_from(limit).unwrap_or(usize::MAX).min(1_000);
        Ok(p.files.iter().skip(start).take(len).cloned().collect())
    }

    /// Withdraws every prompt (shutdown); waiting use cases see a refusal.
    pub(crate) fn withdraw_all(&self) {
        lock(&self.pending).clear();
    }
}

/// The node's `Interaction`: every question becomes a prompt.
#[derive(Debug, Clone)]
pub(crate) struct UiInteraction {
    registry: Arc<PromptRegistry>,
    operation: Option<u64>,
}

impl UiInteraction {
    pub(crate) fn new(registry: Arc<PromptRegistry>, operation: Option<u64>) -> Self {
        Self {
            registry,
            operation,
        }
    }
}

impl Interaction for UiInteraction {
    async fn decide_offer(&self, offer: &IncomingOffer) -> Decision {
        let files: Vec<OfferedFile> = offer
            .files
            .iter()
            .map(|f| OfferedFile {
                name: SafeFileName::sanitize(&f.name).as_str().to_owned(),
                size: f.size,
            })
            .collect();
        let file_count = u32::try_from(files.len()).unwrap_or(u32::MAX);
        let inline: Vec<OfferedFile> = files.iter().take(INLINE_FILES).cloned().collect();
        let transfer_id = offer.transfer_id.to_hex();
        let peer = PeerInfo::from(&offer.peer);
        let code = offer.sas.map(|c| c.as_digits());
        let total_bytes = offer.total_bytes;
        let expires_at_ms = expires_at(self.registry.timeouts.consent);
        let (registration, answer) = self.registry.register(
            |id| Prompt::Offer {
                id,
                transfer_id,
                peer,
                code,
                files: inline,
                file_count,
                total_bytes,
                expires_at_ms,
            },
            files,
        );
        let accepted = answer.await.unwrap_or(false);
        drop(registration);
        if accepted {
            Decision::Accept
        } else {
            Decision::Reject(RejectReason::Declined)
        }
    }

    async fn confirm_sas(&self, prompt: &SasPrompt) -> bool {
        let peer = PeerInfo::from(&prompt.peer);
        let code = prompt.code.as_digits();
        let purpose = match prompt.purpose {
            SasPurpose::Pairing => CodePurpose::Pairing,
            _ => CodePurpose::UnverifiedReceiver,
        };
        let operation = self.operation;
        let expires_at_ms = expires_at(self.registry.timeouts.sas);
        let (registration, answer) = self.registry.register(
            |id| Prompt::Code {
                id,
                operation,
                peer,
                code,
                purpose,
                expires_at_ms,
            },
            Vec::new(),
        );
        let confirmed = answer.await.unwrap_or(false);
        drop(registration);
        confirmed
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    use dropumi_application::events::PeerView;
    use dropumi_domain::device::DeviceName;
    use dropumi_domain::identity::{Fingerprint, PublicKey};
    use dropumi_domain::transfer::{FileEntry, TransferId};
    use dropumi_protocol::sas::SasCode;

    use super::*;
    use crate::error::CallbackError;
    use crate::mailbox::{EventListener, spawn_dispatcher};

    fn peer() -> PeerView {
        PeerView {
            fingerprint: Fingerprint::from_digest([1; 32]),
            public_key: PublicKey::from_bytes([1; 32]),
            trusted: false,
            alias: None,
            claimed_name: DeviceName::sanitize("Peer"),
        }
    }

    fn offer() -> IncomingOffer {
        IncomingOffer {
            transfer_id: TransferId::from_bytes([2; 16]),
            peer: peer(),
            sas: None,
            files: vec![FileEntry {
                name: "a/../b.txt".into(),
                size: 3,
                media_type: None,
                modified: None,
            }],
            total_bytes: 3,
        }
    }

    fn sas() -> SasPrompt {
        SasPrompt {
            peer: peer(),
            code: SasCode::derive(&[1; 32], &[2; 32], &[3; 32]),
            purpose: SasPurpose::Pairing,
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap()
    }

    fn setup() -> (Arc<Mailbox>, Arc<PromptRegistry>) {
        let mailbox = Arc::new(Mailbox::default());
        let registry = PromptRegistry::new(Arc::clone(&mailbox), Timeouts::default());
        (mailbox, registry)
    }

    /// Answers every offer the moment its `PromptOpened` is delivered.
    struct Answerer {
        registry: Arc<PromptRegistry>,
        failures: StdMutex<Vec<DropumiError>>,
    }

    impl EventListener for Answerer {
        fn on_events(&self, events: Vec<NodeEvent>) -> Result<(), CallbackError> {
            for e in events {
                if let NodeEvent::PromptOpened { prompt } = e
                    && let Err(err) = self.registry.answer(prompt.id(), AnswerKind::Offer, true)
                {
                    self.failures.lock().unwrap().push(err);
                }
            }
            Ok(())
        }
    }

    #[test]
    fn prompt_published_only_after_registration() {
        let (mailbox, registry) = setup();
        let answerer = Arc::new(Answerer {
            registry: Arc::clone(&registry),
            failures: StdMutex::default(),
        });
        let dispatcher = spawn_dispatcher(
            Arc::clone(&mailbox),
            Arc::clone(&answerer) as Arc<dyn EventListener>,
        )
        .unwrap();
        let interaction = UiInteraction::new(Arc::clone(&registry), None);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for _ in 0..1_000 {
            let d = runtime.block_on(interaction.decide_offer(&offer()));
            assert_eq!(d, Decision::Accept);
        }
        mailbox.close();
        dispatcher.join().unwrap();
        assert!(answerer.failures.lock().unwrap().is_empty());
    }

    #[test]
    fn decision_without_prompt_never_queued() {
        let (_mailbox, registry) = setup();
        // An answer for a prompt that does not exist yet is refused …
        assert!(matches!(
            registry.answer(1, AnswerKind::Offer, true),
            Err(DropumiError::PromptNotFound)
        ));
        // … and does not apply to the prompt that later gets that id.
        let interaction = UiInteraction::new(Arc::clone(&registry), None);
        let rt = rt();
        let decided = rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(1), interaction.decide_offer(&offer())).await
        });
        assert!(decided.is_err(), "nobody answered, so it waited");
    }

    #[test]
    fn abandoned_prompt_closes_and_rejects_late_answer() {
        let (mailbox, registry) = setup();
        let interaction = UiInteraction::new(Arc::clone(&registry), None);
        let rt = rt();
        let _ = rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(1), interaction.decide_offer(&offer())).await
        });
        assert!(registry.waiting().is_empty());
        assert!(matches!(
            registry.answer(1, AnswerKind::Offer, true),
            Err(DropumiError::PromptNotFound)
        ));
        let events = match mailbox.next(std::time::Instant::now()) {
            crate::mailbox::Next::Deliver(b) => b,
            other => panic!("{other:?}"),
        };
        assert!(events.contains(&NodeEvent::PromptClosed {
            id: 1,
            reason: PromptCloseReason::Withdrawn,
        }));
    }

    #[test]
    fn prompt_ids_never_reused_and_names_are_sanitized() {
        let (_mailbox, registry) = setup();
        let interaction = UiInteraction::new(Arc::clone(&registry), None);
        let rt = rt();
        let mut ids = Vec::new();
        for _ in 0..3 {
            let _ = rt.block_on(async {
                tokio::time::timeout(
                    Duration::from_millis(10),
                    interaction.decide_offer(&offer()),
                )
                .await
            });
        }
        rt.block_on(async {
            let o = offer();
            let fut = interaction.decide_offer(&o);
            tokio::pin!(fut);
            let _ = tokio::time::timeout(Duration::from_millis(1), &mut fut).await;
            let waiting = registry.waiting();
            ids.extend(waiting.iter().map(Prompt::id));
            if let Some(Prompt::Offer { files, .. }) = waiting.first() {
                assert_eq!(files[0].name, SafeFileName::sanitize("a/../b.txt").as_str());
                assert!(!files[0].name.contains('/'));
            }
        });
        assert_eq!(ids, vec![4], "three earlier prompts used ids 1 to 3");
    }

    #[test]
    fn offer_answer_rejected_for_code_prompt() {
        let (_mailbox, registry) = setup();
        let interaction = UiInteraction::new(Arc::clone(&registry), Some(7));
        let rt = rt();
        let confirmed = rt.block_on(async {
            let p = sas();
            let fut = interaction.confirm_sas(&p);
            tokio::pin!(fut);
            assert!(
                tokio::time::timeout(Duration::from_millis(1), &mut fut)
                    .await
                    .is_err()
            );
            let Some(Prompt::Code {
                id,
                operation,
                code,
                ..
            }) = registry.waiting().first().cloned()
            else {
                panic!("no code prompt");
            };
            assert_eq!(operation, Some(7));
            assert_eq!(code, sas().code.as_digits());
            assert!(matches!(
                registry.answer(id, AnswerKind::Offer, true),
                Err(DropumiError::PromptKindMismatch)
            ));
            // Still waiting; the right kind of answer works.
            registry.answer(id, AnswerKind::Code, true).unwrap();
            fut.await
        });
        assert!(confirmed);
    }

    #[test]
    fn withdraw_all_refuses_waiting_prompts() {
        let (_mailbox, registry) = setup();
        let interaction = UiInteraction::new(Arc::clone(&registry), None);
        let rt = rt();
        let decision = rt.block_on(async {
            let o = offer();
            let fut = interaction.decide_offer(&o);
            tokio::pin!(fut);
            let _ = tokio::time::timeout(Duration::from_millis(1), &mut fut).await;
            registry.withdraw_all();
            fut.await
        });
        assert_eq!(decision, Decision::Reject(RejectReason::Declined));
    }
}
