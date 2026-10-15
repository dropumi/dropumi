// SPDX-License-Identifier: MPL-2.0
//! Connection admission: bounded unauthenticated and authenticated work.
//!
//! - at most 16 unauthenticated connections, 4 per remote IP (excess
//!   connections are dropped immediately, without a queue);
//! - at most 4 authenticated sessions (excess get `BUSY` at the preamble);
//! - at most 1 active session per peer fingerprint (excess get `BUSY`).
//!
//! Permits release their slot when dropped.

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::{Arc, Mutex, PoisonError};

use dropumi_domain::identity::Fingerprint;

use crate::config::AdmissionLimits;

#[derive(Debug, Default)]
struct State {
    preauth: usize,
    per_ip: HashMap<IpAddr, usize>,
    sessions: usize,
    fingerprints: HashSet<Fingerprint>,
}

/// Shared admission state.
#[derive(Debug, Clone)]
pub struct Admission {
    state: Arc<Mutex<State>>,
    limits: AdmissionLimits,
}

impl Admission {
    /// New, empty admission state.
    #[must_use]
    pub fn new(limits: AdmissionLimits) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            limits,
        }
    }

    fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        let mut guard = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        f(&mut guard)
    }

    /// Admits an unauthenticated connection from `ip`, if a slot is free.
    #[must_use]
    pub fn try_preauth(&self, ip: IpAddr) -> Option<PreAuthPermit> {
        let ok = self.with(|s| {
            let per_ip = s.per_ip.get(&ip).copied().unwrap_or(0);
            if s.preauth >= self.limits.preauth_global || per_ip >= self.limits.preauth_per_ip {
                return false;
            }
            s.preauth += 1;
            s.per_ip.insert(ip, per_ip + 1);
            true
        });
        ok.then(|| PreAuthPermit {
            admission: self.clone(),
            ip,
        })
    }

    /// Whether every session slot is taken.
    #[must_use]
    pub fn sessions_full(&self) -> bool {
        self.with(|s| s.sessions >= self.limits.sessions)
    }

    /// Admits an authenticated session for `fp`, if a slot is free and the
    /// fingerprint has no active session.
    #[must_use]
    pub fn try_session(&self, fp: Fingerprint) -> Option<SessionPermit> {
        let ok = self.with(|s| {
            if s.sessions >= self.limits.sessions || s.fingerprints.contains(&fp) {
                return false;
            }
            s.sessions += 1;
            s.fingerprints.insert(fp);
            true
        });
        ok.then(|| SessionPermit {
            admission: self.clone(),
            fp,
        })
    }
}

/// An unauthenticated connection slot.
#[derive(Debug)]
pub struct PreAuthPermit {
    admission: Admission,
    ip: IpAddr,
}

impl Drop for PreAuthPermit {
    fn drop(&mut self) {
        let ip = self.ip;
        self.admission.with(|s| {
            s.preauth = s.preauth.saturating_sub(1);
            if let Some(n) = s.per_ip.get_mut(&ip) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    s.per_ip.remove(&ip);
                }
            }
        });
    }
}

/// An authenticated session slot.
#[derive(Debug)]
pub struct SessionPermit {
    admission: Admission,
    fp: Fingerprint,
}

impl Drop for SessionPermit {
    fn drop(&mut self) {
        let fp = self.fp;
        self.admission.with(|s| {
            s.sessions = s.sessions.saturating_sub(1);
            s.fingerprints.remove(&fp);
        });
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    #[test]
    fn admission_limits_global_per_ip_and_session() {
        let a = Admission::new(AdmissionLimits::default());
        let ip = |n| IpAddr::V4(Ipv4Addr::new(10, 0, 0, n));
        // 4 per IP.
        let same: Vec<_> = (0..4).map(|_| a.try_preauth(ip(1)).unwrap()).collect();
        assert!(a.try_preauth(ip(1)).is_none(), "5th from one IP dropped");
        // 16 in total.
        let mut others: Vec<_> = (2..5)
            .flat_map(|n| (0..4).map(move |_| n))
            .map(|n| a.try_preauth(ip(n)).unwrap())
            .collect();
        assert_eq!(same.len() + others.len(), 16);
        assert!(a.try_preauth(ip(9)).is_none(), "17th dropped");
        others.pop();
        assert!(a.try_preauth(ip(9)).is_some(), "slot released on drop");
        drop(same);
        assert!(a.try_preauth(ip(1)).is_some());

        // 4 sessions; 1 per fingerprint.
        let fp = |b| Fingerprint::from_digest([b; 32]);
        let s1 = a.try_session(fp(1)).unwrap();
        assert!(
            a.try_session(fp(1)).is_none(),
            "same fingerprint twice is busy"
        );
        let _s2 = a.try_session(fp(2)).unwrap();
        let _s3 = a.try_session(fp(3)).unwrap();
        let _s4 = a.try_session(fp(4)).unwrap();
        assert!(a.sessions_full());
        assert!(a.try_session(fp(5)).is_none(), "5th session busy");
        drop(s1);
        assert!(!a.sessions_full());
        assert!(a.try_session(fp(1)).is_some());
    }

    #[test]
    fn one_session_per_fingerprint_regardless_of_session_slots() {
        // Protocol v1 §17.2 fixes the per-fingerprint limit at 1; more
        // session slots must not loosen it.
        let a = Admission::new(AdmissionLimits {
            sessions: 8,
            ..AdmissionLimits::default()
        });
        let fp = |b| Fingerprint::from_digest([b; 32]);
        let first = a.try_session(fp(1)).unwrap();
        assert!(
            a.try_session(fp(1)).is_none(),
            "a second session for the same fingerprint is busy"
        );
        assert!(
            a.try_session(fp(2)).is_some(),
            "another fingerprint is admitted"
        );
        drop(first);
        assert!(
            a.try_session(fp(1)).is_some(),
            "the fingerprint is admitted again once its session ends"
        );
    }
}
