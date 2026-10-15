// SPDX-License-Identifier: MPL-2.0
//! Transfer progress with enforced bounds.

/// Progress of a transfer. Invariant: `bytes_done ≤ bytes_total` and
/// `files_done ≤ files_total`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransferProgress {
    bytes_done: u64,
    bytes_total: u64,
    files_done: u32,
    files_total: u32,
    current_file: Option<u32>,
}

/// A progress update that would break an invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("progress update exceeds the transfer totals")]
pub struct ProgressError;

impl TransferProgress {
    /// Starts at zero.
    #[must_use]
    pub const fn new(bytes_total: u64, files_total: u32) -> Self {
        Self {
            bytes_done: 0,
            bytes_total,
            files_done: 0,
            files_total,
            current_file: None,
        }
    }

    /// Records transferred bytes.
    ///
    /// # Errors
    /// [`ProgressError`] if the total would be exceeded (state unchanged).
    pub fn add_bytes(&mut self, n: u64) -> Result<(), ProgressError> {
        let next = self
            .bytes_done
            .checked_add(n)
            .filter(|v| *v <= self.bytes_total)
            .ok_or(ProgressError)?;
        self.bytes_done = next;
        Ok(())
    }

    /// Accounts for bytes of a file that will not be transferred (aborted or
    /// failed), so the byte total stays reachable.
    ///
    /// # Errors
    /// [`ProgressError`] if the total would be exceeded (state unchanged).
    pub fn skip_bytes(&mut self, n: u64) -> Result<(), ProgressError> {
        self.add_bytes(n)
    }

    /// Marks the start of file `index`.
    pub fn start_file(&mut self, index: u32) {
        self.current_file = Some(index);
    }

    /// Marks one more file as settled.
    ///
    /// # Errors
    /// [`ProgressError`] if every file is already settled (state unchanged).
    pub fn finish_file(&mut self) -> Result<(), ProgressError> {
        if self.files_done >= self.files_total {
            return Err(ProgressError);
        }
        self.files_done += 1;
        self.current_file = None;
        Ok(())
    }

    /// Bytes transferred so far.
    #[must_use]
    pub const fn bytes_done(&self) -> u64 {
        self.bytes_done
    }

    /// Total bytes.
    #[must_use]
    pub const fn bytes_total(&self) -> u64 {
        self.bytes_total
    }

    /// Files settled so far.
    #[must_use]
    pub const fn files_done(&self) -> u32 {
        self.files_done
    }

    /// Total files.
    #[must_use]
    pub const fn files_total(&self) -> u32 {
        self.files_total
    }

    /// File currently being transferred.
    #[must_use]
    pub const fn current_file(&self) -> Option<u32> {
        self.current_file
    }

    /// Completion ratio in `[0, 1]`; 1 for empty transfers.
    #[must_use]
    pub fn fraction(&self) -> f64 {
        if self.bytes_total == 0 {
            if self.files_total == 0 || self.files_done == self.files_total {
                1.0
            } else {
                0.0
            }
        } else {
            #[allow(clippy::cast_precision_loss)]
            let f = self.bytes_done as f64 / self.bytes_total as f64;
            f
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_invariants() {
        let mut p = TransferProgress::new(10, 2);
        p.add_bytes(4).unwrap();
        assert_eq!(p.add_bytes(7), Err(ProgressError));
        assert_eq!(p.bytes_done(), 4);
        p.add_bytes(6).unwrap();
        assert_eq!(p.add_bytes(1), Err(ProgressError));
        p.start_file(0);
        assert_eq!(p.current_file(), Some(0));
        p.finish_file().unwrap();
        p.finish_file().unwrap();
        assert_eq!(p.finish_file(), Err(ProgressError));
        assert_eq!(p.files_done(), 2);
        assert!((p.fraction() - 1.0).abs() < f64::EPSILON);
        assert_eq!(TransferProgress::new(0, 1).fraction(), 0.0);
        assert_eq!(
            TransferProgress::new(u64::MAX, 1).add_bytes(u64::MAX),
            Ok(())
        );
    }
}
