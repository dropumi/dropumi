// SPDX-License-Identifier: MPL-2.0
//! Randomness from the operating system.

use dropumi_domain::transfer::TransferId;

use crate::error::ProtocolError;

/// Fills `buf` from the OS random number generator.
///
/// # Errors
/// [`ProtocolError::Random`] if the generator fails.
pub fn fill(buf: &mut [u8]) -> Result<(), ProtocolError> {
    getrandom::fill(buf).map_err(|_| ProtocolError::Random)
}

/// A fresh random transfer id.
///
/// # Errors
/// [`ProtocolError::Random`] if the generator fails.
pub fn transfer_id() -> Result<TransferId, ProtocolError> {
    let mut b = [0u8; 16];
    fill(&mut b)?;
    Ok(TransferId::from_bytes(b))
}

/// A fresh 32-byte SAS nonce.
///
/// # Errors
/// [`ProtocolError::Random`] if the generator fails.
pub fn sas_nonce() -> Result<[u8; 32], ProtocolError> {
    let mut b = [0u8; 32];
    fill(&mut b)?;
    Ok(b)
}
