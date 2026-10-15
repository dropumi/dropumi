// SPDX-License-Identifier: MPL-2.0
//! Control messages: `[type uint, {uint ⇒ value}]` in strict CBOR.
//!
//! Unknown keys are skipped. Unknown types 0–127 are fatal
//! ([`ProtocolError::Unsupported`]); types 128–255 are ignorable.

use dropumi_domain::device::DeviceKind;
use dropumi_domain::filename::validate_wire_name;
use dropumi_domain::limits::{MAX_FILE_SIZE, MAX_FILES_PER_TRANSFER, MAX_WIRE_NAME_BYTES};
use dropumi_domain::peer::Intent;
use dropumi_domain::transfer::{AbortReason, CancelReason, FileEntry, RejectReason, TransferId};
use minicbor::Encoder;

use crate::codec::{MapReader, Reader};
use crate::error::ProtocolError;
use crate::limits::{
    BLAKE3_LEN, HASH_ALG_BLAKE3, MAX_DEVICE_NAME_WIRE, MAX_ERROR_DETAIL, MAX_FEATURES,
    MAX_HASH_ALGS, MAX_MEDIA_TYPE, MAX_SOFTWARE,
};

/// Which side sent a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// The side that opened the connection (the sender of files).
    Initiator,
    /// The side that accepted the connection (the receiver of files).
    Responder,
}

/// Protocol error codes carried by `Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    /// The peer broke the protocol.
    ProtocolViolation,
    /// An unknown, non-ignorable message type.
    UnsupportedMessage,
    /// A limit was exceeded.
    LimitExceeded,
    /// The session is not authorized.
    NotAuthorized,
    /// Pairing failed.
    PairingFailed,
    /// The device is busy.
    Busy,
    /// A timeout expired.
    Timeout,
    /// An internal error.
    Internal,
    /// A code this version does not know.
    Unknown(u64),
}

impl ErrorCode {
    /// Wire code.
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::ProtocolViolation => 1,
            Self::UnsupportedMessage => 2,
            Self::LimitExceeded => 3,
            Self::NotAuthorized => 4,
            Self::PairingFailed => 5,
            Self::Busy => 6,
            Self::Timeout => 7,
            Self::Internal => 8,
            Self::Unknown(c) => c,
        }
    }

    /// Decodes a wire code.
    #[must_use]
    pub const fn from_code(code: u64) -> Self {
        match code {
            1 => Self::ProtocolViolation,
            2 => Self::UnsupportedMessage,
            3 => Self::LimitExceeded,
            4 => Self::NotAuthorized,
            5 => Self::PairingFailed,
            6 => Self::Busy,
            7 => Self::Timeout,
            8 => Self::Internal,
            c => Self::Unknown(c),
        }
    }

    /// The fixed English phrase that is the only permitted `detail` for
    /// this code. Implementations MUST NOT send paths, OS error text, file
    /// names or other local data.
    #[must_use]
    pub const fn wire_detail(self) -> &'static str {
        match self {
            Self::ProtocolViolation => "protocol violation",
            Self::UnsupportedMessage => "unsupported message",
            Self::LimitExceeded => "limit exceeded",
            Self::NotAuthorized => "not authorized",
            Self::PairingFailed => "pairing failed",
            Self::Busy => "busy",
            Self::Timeout => "timeout",
            Self::Internal | Self::Unknown(_) => "internal error",
        }
    }
}

/// Receiver's per-file result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AckStatus {
    /// Content fsynced and visible under its final name.
    Stored,
    /// Hash mismatch; nothing kept.
    IntegrityFailed,
    /// Write failure; nothing kept.
    WriteFailed,
    /// The sender aborted the file; nothing kept.
    Aborted,
}

impl AckStatus {
    /// Wire code.
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::Stored => 0,
            Self::IntegrityFailed => 1,
            Self::WriteFailed => 2,
            Self::Aborted => 3,
        }
    }

    /// Decodes a wire code; unknown values are a protocol violation.
    ///
    /// # Errors
    /// [`ProtocolError::Violation`] for unknown statuses.
    pub const fn from_code(code: u64) -> Result<Self, ProtocolError> {
        match code {
            0 => Ok(Self::Stored),
            1 => Ok(Self::IntegrityFailed),
            2 => Ok(Self::WriteFailed),
            3 => Ok(Self::Aborted),
            _ => Err(ProtocolError::Violation("unknown ack status")),
        }
    }

    /// Stable machine-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stored => "stored",
            Self::IntegrityFailed => "integrity_failed",
            Self::WriteFailed => "write_failed",
            Self::Aborted => "aborted",
        }
    }
}

/// A file content hash.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileHash {
    /// BLAKE3-256 (algorithm id 1).
    Blake3([u8; BLAKE3_LEN]),
}

impl std::fmt::Debug for FileHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blake3(d) => {
                f.write_str("Blake3(")?;
                for b in d {
                    write!(f, "{b:02x}")?;
                }
                f.write_str(")")
            }
        }
    }
}

/// The first transport message of each side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    /// Self-declared device name (1..=128 bytes, untrusted).
    pub device_name: String,
    /// Self-declared device kind.
    pub device_kind: DeviceKind,
    /// Software identifier (≤64 bytes).
    pub software: Option<String>,
    /// Optional feature ids (v1 defines none).
    pub features: Vec<u16>,
    /// Supported hash algorithms; must contain 1 (BLAKE3).
    pub hash_algs: Vec<u16>,
    /// Session intent (initiator only).
    pub intent: Option<Intent>,
    /// SAS nonce reveal (responder only).
    pub sas_reveal: Option<[u8; 32]>,
}

/// A protocol v1 control message.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Message {
    /// Type 1.
    Hello(Hello),
    /// Type 2: the local user's pairing decision.
    PairResult {
        /// Whether the user confirmed the SAS.
        confirmed: bool,
    },
    /// Type 8: fatal error; the sender closes.
    Error {
        /// Error code.
        code: ErrorCode,
        /// Fixed phrase for the code (untrusted display text when received).
        detail: Option<String>,
    },
    /// Type 9: cancel the session.
    Cancel {
        /// Why.
        reason: CancelReason,
    },
    /// Type 16: offer header.
    Offer {
        /// Transfer id.
        transfer_id: TransferId,
        /// Number of files (1..=10,000).
        file_count: u32,
        /// Sum of file sizes.
        total_size: u64,
    },
    /// Type 17: a batch of offered files.
    OfferFiles {
        /// Transfer id.
        transfer_id: TransferId,
        /// Index of the first file in this batch.
        first_index: u32,
        /// Files.
        files: Vec<FileEntry>,
    },
    /// Type 18.
    Accept {
        /// Transfer id.
        transfer_id: TransferId,
    },
    /// Type 19.
    Reject {
        /// Transfer id.
        transfer_id: TransferId,
        /// Reason.
        reason: RejectReason,
    },
    /// Type 20: file content follows in DATA frames.
    FileStart {
        /// File index (must be the next file).
        index: u32,
        /// Start offset (must be 0 in v1; reserved for resume).
        offset: u64,
    },
    /// Type 21: end of file content.
    FileEnd {
        /// File index.
        index: u32,
        /// Content hash.
        hash: FileHash,
    },
    /// Type 22: the sender abandons a file.
    FileAbort {
        /// File index.
        index: u32,
        /// Reason.
        reason: AbortReason,
    },
    /// Type 23: the receiver's per-file result.
    FileAck {
        /// File index.
        index: u32,
        /// Result.
        status: AckStatus,
    },
    /// Type 24: the sender has nothing more to send.
    Complete {
        /// Transfer id.
        transfer_id: TransferId,
    },
    /// Type 25: everything stored is durable.
    CompleteAck {
        /// Transfer id.
        transfer_id: TransferId,
        /// Number of files stored.
        stored_count: u32,
    },
}

/// Type code of each message.
mod ty {
    pub const HELLO: u64 = 1;
    pub const PAIR_RESULT: u64 = 2;
    pub const ERROR: u64 = 8;
    pub const CANCEL: u64 = 9;
    pub const OFFER: u64 = 16;
    pub const OFFER_FILES: u64 = 17;
    pub const ACCEPT: u64 = 18;
    pub const REJECT: u64 = 19;
    pub const FILE_START: u64 = 20;
    pub const FILE_END: u64 = 21;
    pub const FILE_ABORT: u64 = 22;
    pub const FILE_ACK: u64 = 23;
    pub const COMPLETE: u64 = 24;
    pub const COMPLETE_ACK: u64 = 25;
    pub const FIRST_IGNORABLE: u64 = 128;
}

impl Message {
    /// Wire type code.
    #[must_use]
    pub const fn type_code(&self) -> u64 {
        match self {
            Self::Hello(_) => ty::HELLO,
            Self::PairResult { .. } => ty::PAIR_RESULT,
            Self::Error { .. } => ty::ERROR,
            Self::Cancel { .. } => ty::CANCEL,
            Self::Offer { .. } => ty::OFFER,
            Self::OfferFiles { .. } => ty::OFFER_FILES,
            Self::Accept { .. } => ty::ACCEPT,
            Self::Reject { .. } => ty::REJECT,
            Self::FileStart { .. } => ty::FILE_START,
            Self::FileEnd { .. } => ty::FILE_END,
            Self::FileAbort { .. } => ty::FILE_ABORT,
            Self::FileAck { .. } => ty::FILE_ACK,
            Self::Complete { .. } => ty::COMPLETE,
            Self::CompleteAck { .. } => ty::COMPLETE_ACK,
        }
    }

    /// The side allowed to send this message (`None`: both).
    #[must_use]
    pub const fn sender(&self) -> Option<Role> {
        match self {
            Self::Hello(_) | Self::PairResult { .. } | Self::Error { .. } | Self::Cancel { .. } => {
                None
            }
            Self::Offer { .. }
            | Self::OfferFiles { .. }
            | Self::FileStart { .. }
            | Self::FileEnd { .. }
            | Self::FileAbort { .. }
            | Self::Complete { .. } => Some(Role::Initiator),
            Self::Accept { .. }
            | Self::Reject { .. }
            | Self::FileAck { .. }
            | Self::CompleteAck { .. } => Some(Role::Responder),
        }
    }

    /// An `Error` message with the code's fixed detail phrase.
    #[must_use]
    pub fn error(code: ErrorCode) -> Self {
        Self::Error {
            code,
            detail: Some(code.wire_detail().to_owned()),
        }
    }
}

/// Result of decoding a control message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    /// A known message.
    Message(Message),
    /// An ignorable message (type 128–255) that was skipped.
    Ignored(u8),
}

type EncResult = Result<(), minicbor::encode::Error<std::convert::Infallible>>;

/// Appends the CBOR encoding of `msg` to `out`.
///
/// # Errors
/// [`ProtocolError::Encode`] (not expected for in-memory buffers).
pub fn encode(msg: &Message, out: &mut Vec<u8>) -> Result<(), ProtocolError> {
    let mut e = Encoder::new(out);
    encode_inner(msg, &mut e).map_err(|_| ProtocolError::Encode)
}

fn encode_inner(msg: &Message, e: &mut Encoder<&mut Vec<u8>>) -> EncResult {
    e.array(2)?.u64(msg.type_code())?;
    match msg {
        Message::Hello(h) => {
            let n: u64 = [
                true,
                true,
                true,
                h.software.is_some(),
                !h.features.is_empty(),
                h.intent.is_some(),
                h.sas_reveal.is_some(),
            ]
            .iter()
            .map(|b| u64::from(*b))
            .sum();
            e.map(n)?;
            e.u64(1)?.str(&h.device_name)?;
            e.u64(2)?.u64(h.device_kind.code())?;
            if let Some(s) = &h.software {
                e.u64(3)?.str(s)?;
            }
            if !h.features.is_empty() {
                e.u64(4)?.array(h.features.len() as u64)?;
                for f in &h.features {
                    e.u64(u64::from(*f))?;
                }
            }
            e.u64(5)?.array(h.hash_algs.len() as u64)?;
            for a in &h.hash_algs {
                e.u64(u64::from(*a))?;
            }
            if let Some(i) = h.intent {
                e.u64(6)?.u64(i.code())?;
            }
            if let Some(r) = &h.sas_reveal {
                e.u64(7)?.bytes(r)?;
            }
        }
        Message::PairResult { confirmed } => {
            e.map(1)?.u64(1)?.bool(*confirmed)?;
        }
        Message::Error { code, detail } => {
            e.map(if detail.is_some() { 2 } else { 1 })?;
            e.u64(1)?.u64(code.code())?;
            if let Some(d) = detail {
                e.u64(2)?.str(d)?;
            }
        }
        Message::Cancel { reason } => {
            e.map(1)?.u64(1)?.u64(reason.code())?;
        }
        Message::Offer {
            transfer_id,
            file_count,
            total_size,
        } => {
            e.map(3)?;
            e.u64(1)?.bytes(transfer_id.as_bytes())?;
            e.u64(2)?.u64(u64::from(*file_count))?;
            e.u64(3)?.u64(*total_size)?;
        }
        Message::OfferFiles {
            transfer_id,
            first_index,
            files,
        } => {
            e.map(3)?;
            e.u64(1)?.bytes(transfer_id.as_bytes())?;
            e.u64(2)?.u64(u64::from(*first_index))?;
            e.u64(3)?.array(files.len() as u64)?;
            for f in files {
                encode_file_entry(f, e)?;
            }
        }
        Message::Accept { transfer_id } | Message::Complete { transfer_id } => {
            e.map(1)?.u64(1)?.bytes(transfer_id.as_bytes())?;
        }
        Message::Reject {
            transfer_id,
            reason,
        } => {
            e.map(2)?;
            e.u64(1)?.bytes(transfer_id.as_bytes())?;
            e.u64(2)?.u64(reason.code())?;
        }
        Message::FileStart { index, offset } => {
            e.map(2)?;
            e.u64(1)?.u64(u64::from(*index))?;
            e.u64(2)?.u64(*offset)?;
        }
        Message::FileEnd { index, hash } => {
            e.map(2)?;
            e.u64(1)?.u64(u64::from(*index))?;
            let FileHash::Blake3(d) = hash;
            e.u64(2)?
                .array(2)?
                .u64(u64::from(HASH_ALG_BLAKE3))?
                .bytes(d)?;
        }
        Message::FileAbort { index, reason } => {
            e.map(2)?;
            e.u64(1)?.u64(u64::from(*index))?;
            e.u64(2)?.u64(reason.code())?;
        }
        Message::FileAck { index, status } => {
            e.map(2)?;
            e.u64(1)?.u64(u64::from(*index))?;
            e.u64(2)?.u64(status.code())?;
        }
        Message::CompleteAck {
            transfer_id,
            stored_count,
        } => {
            e.map(2)?;
            e.u64(1)?.bytes(transfer_id.as_bytes())?;
            e.u64(2)?.u64(u64::from(*stored_count))?;
        }
    }
    Ok(())
}

fn encode_file_entry(f: &FileEntry, e: &mut Encoder<&mut Vec<u8>>) -> EncResult {
    let n: u64 = [true, true, f.media_type.is_some(), f.modified.is_some()]
        .iter()
        .map(|b| u64::from(*b))
        .sum();
    e.map(n)?;
    e.u64(1)?.str(&f.name)?;
    e.u64(2)?.u64(f.size)?;
    if let Some(m) = &f.media_type {
        e.u64(3)?.str(m)?;
    }
    if let Some(t) = f.modified {
        e.u64(4)?.i64(t)?;
    }
    Ok(())
}

/// Encoded size of one file entry (for offer batching).
///
/// # Errors
/// [`ProtocolError::Encode`] (not expected).
pub fn file_entry_encoded_len(f: &FileEntry) -> Result<usize, ProtocolError> {
    let mut buf = Vec::new();
    let mut e = Encoder::new(&mut buf);
    encode_file_entry(f, &mut e).map_err(|_| ProtocolError::Encode)?;
    Ok(buf.len())
}

/// Decodes one control message sent by `from`.
///
/// # Errors
/// [`ProtocolError::Malformed`], [`ProtocolError::LimitExceeded`],
/// [`ProtocolError::Unsupported`] or [`ProtocolError::Violation`] (wrong
/// direction, bad values).
pub fn decode(buf: &[u8], from: Role) -> Result<Decoded, ProtocolError> {
    let mut r = Reader::new(buf);
    if r.array_len(2, "message array")? != 2 {
        return Err(ProtocolError::Malformed(
            "message must be a 2-element array",
        ));
    }
    let ty = r.uint_max(255, "message type")?;
    let ty8 = u8::try_from(ty).map_err(|_| ProtocolError::Malformed("message type"))?;
    let mut m = r.map(2)?;
    let msg = match ty {
        ty::HELLO => Message::Hello(decode_hello(&mut r, &mut m, from)?),
        ty::PAIR_RESULT => {
            let mut confirmed = None;
            while let Some(k) = m.next_key(&mut r)? {
                match k {
                    1 => confirmed = Some(r.bool()?),
                    _ => r.skip_value(m.value_depth())?,
                }
            }
            Message::PairResult {
                confirmed: confirmed.ok_or(missing())?,
            }
        }
        ty::ERROR => {
            let (mut code, mut detail) = (None, None);
            while let Some(k) = m.next_key(&mut r)? {
                match k {
                    1 => code = Some(ErrorCode::from_code(r.uint()?)),
                    2 => detail = Some(r.text(MAX_ERROR_DETAIL, "error detail")?.to_owned()),
                    _ => r.skip_value(m.value_depth())?,
                }
            }
            Message::Error {
                code: code.ok_or(missing())?,
                detail,
            }
        }
        ty::CANCEL => {
            let mut reason = None;
            while let Some(k) = m.next_key(&mut r)? {
                match k {
                    1 => reason = Some(CancelReason::from_code(r.uint()?)),
                    _ => r.skip_value(m.value_depth())?,
                }
            }
            Message::Cancel {
                reason: reason.ok_or(missing())?,
            }
        }
        ty::OFFER => {
            let (mut id, mut count, mut total) = (None, None, None);
            while let Some(k) = m.next_key(&mut r)? {
                match k {
                    1 => id = Some(transfer_id(&mut r)?),
                    2 => count = Some(file_count(&mut r)?),
                    3 => total = Some(r.uint_max(MAX_FILE_SIZE, "total size")?),
                    _ => r.skip_value(m.value_depth())?,
                }
            }
            Message::Offer {
                transfer_id: id.ok_or(missing())?,
                file_count: count.ok_or(missing())?,
                total_size: total.ok_or(missing())?,
            }
        }
        ty::OFFER_FILES => {
            let (mut id, mut first, mut files) = (None, None, None);
            while let Some(k) = m.next_key(&mut r)? {
                match k {
                    1 => id = Some(transfer_id(&mut r)?),
                    2 => first = Some(file_index(&mut r)?),
                    3 => {
                        let n = r.array_len(MAX_FILES_PER_TRANSFER as u64, "files per batch")?;
                        if n == 0 {
                            return Err(ProtocolError::Violation("empty file batch"));
                        }
                        let mut v = Vec::with_capacity(n.min(1024));
                        for _ in 0..n {
                            v.push(decode_file_entry(
                                &mut r,
                                m.value_depth().saturating_add(1),
                            )?);
                        }
                        files = Some(v);
                    }
                    _ => r.skip_value(m.value_depth())?,
                }
            }
            Message::OfferFiles {
                transfer_id: id.ok_or(missing())?,
                first_index: first.ok_or(missing())?,
                files: files.ok_or(missing())?,
            }
        }
        ty::ACCEPT | ty::COMPLETE => {
            let mut id = None;
            while let Some(k) = m.next_key(&mut r)? {
                match k {
                    1 => id = Some(transfer_id(&mut r)?),
                    _ => r.skip_value(m.value_depth())?,
                }
            }
            let transfer_id = id.ok_or(missing())?;
            if ty == ty::ACCEPT {
                Message::Accept { transfer_id }
            } else {
                Message::Complete { transfer_id }
            }
        }
        ty::REJECT => {
            let (mut id, mut reason) = (None, None);
            while let Some(k) = m.next_key(&mut r)? {
                match k {
                    1 => id = Some(transfer_id(&mut r)?),
                    2 => reason = Some(RejectReason::from_code(r.uint()?)),
                    _ => r.skip_value(m.value_depth())?,
                }
            }
            Message::Reject {
                transfer_id: id.ok_or(missing())?,
                reason: reason.ok_or(missing())?,
            }
        }
        ty::FILE_START => {
            let (mut index, mut offset) = (None, None);
            while let Some(k) = m.next_key(&mut r)? {
                match k {
                    1 => index = Some(file_index(&mut r)?),
                    2 => offset = Some(r.uint_max(MAX_FILE_SIZE, "offset")?),
                    _ => r.skip_value(m.value_depth())?,
                }
            }
            Message::FileStart {
                index: index.ok_or(missing())?,
                offset: offset.ok_or(missing())?,
            }
        }
        ty::FILE_END => {
            let (mut index, mut hash) = (None, None);
            while let Some(k) = m.next_key(&mut r)? {
                match k {
                    1 => index = Some(file_index(&mut r)?),
                    2 => hash = Some(decode_hash(&mut r)?),
                    _ => r.skip_value(m.value_depth())?,
                }
            }
            Message::FileEnd {
                index: index.ok_or(missing())?,
                hash: hash.ok_or(missing())?,
            }
        }
        ty::FILE_ABORT => {
            let (mut index, mut reason) = (None, None);
            while let Some(k) = m.next_key(&mut r)? {
                match k {
                    1 => index = Some(file_index(&mut r)?),
                    2 => reason = Some(AbortReason::from_code(r.uint()?)),
                    _ => r.skip_value(m.value_depth())?,
                }
            }
            Message::FileAbort {
                index: index.ok_or(missing())?,
                reason: reason.ok_or(missing())?,
            }
        }
        ty::FILE_ACK => {
            let (mut index, mut status) = (None, None);
            while let Some(k) = m.next_key(&mut r)? {
                match k {
                    1 => index = Some(file_index(&mut r)?),
                    2 => status = Some(AckStatus::from_code(r.uint()?)?),
                    _ => r.skip_value(m.value_depth())?,
                }
            }
            Message::FileAck {
                index: index.ok_or(missing())?,
                status: status.ok_or(missing())?,
            }
        }
        ty::COMPLETE_ACK => {
            let (mut id, mut stored) = (None, None);
            while let Some(k) = m.next_key(&mut r)? {
                match k {
                    1 => id = Some(transfer_id(&mut r)?),
                    2 => stored = Some(file_count_or_zero(&mut r)?),
                    _ => r.skip_value(m.value_depth())?,
                }
            }
            Message::CompleteAck {
                transfer_id: id.ok_or(missing())?,
                stored_count: stored.ok_or(missing())?,
            }
        }
        t if t >= ty::FIRST_IGNORABLE => {
            while m.next_key(&mut r)?.is_some() {
                r.skip_value(m.value_depth())?;
            }
            r.finish()?;
            return Ok(Decoded::Ignored(ty8));
        }
        _ => {
            // Validate the body strictly before reporting the unknown type.
            while m.next_key(&mut r)?.is_some() {
                r.skip_value(m.value_depth())?;
            }
            r.finish()?;
            return Err(ProtocolError::Unsupported(ty8));
        }
    };
    r.finish()?;
    if let Some(required) = msg.sender()
        && required != from
    {
        return Err(ProtocolError::Violation(
            "message sent in the wrong direction",
        ));
    }
    Ok(Decoded::Message(msg))
}

const fn missing() -> ProtocolError {
    ProtocolError::Malformed("missing required key")
}

fn transfer_id(r: &mut Reader<'_>) -> Result<TransferId, ProtocolError> {
    let b = r.bytes_exact(16)?;
    let arr: [u8; 16] = b
        .try_into()
        .map_err(|_| ProtocolError::Malformed("transfer id"))?;
    Ok(TransferId::from_bytes(arr))
}

fn file_index(r: &mut Reader<'_>) -> Result<u32, ProtocolError> {
    let max = u32::try_from(MAX_FILES_PER_TRANSFER.saturating_sub(1)).unwrap_or(u32::MAX);
    r.u32_max(max, "file index")
}

fn file_count(r: &mut Reader<'_>) -> Result<u32, ProtocolError> {
    let n = file_count_or_zero(r)?;
    if n == 0 {
        return Err(ProtocolError::Violation("offer without files"));
    }
    Ok(n)
}

fn file_count_or_zero(r: &mut Reader<'_>) -> Result<u32, ProtocolError> {
    let max = u32::try_from(MAX_FILES_PER_TRANSFER).unwrap_or(u32::MAX);
    r.u32_max(max, "file count")
}

fn decode_hash(r: &mut Reader<'_>) -> Result<FileHash, ProtocolError> {
    if r.array_len(2, "hash")? != 2 {
        return Err(ProtocolError::Malformed("hash must be [alg, digest]"));
    }
    let alg = r.uint()?;
    if alg != u64::from(HASH_ALG_BLAKE3) {
        return Err(ProtocolError::Violation("unsupported hash algorithm"));
    }
    Ok(FileHash::Blake3(r.bytes32()?))
}

fn decode_file_entry(r: &mut Reader<'_>, depth: usize) -> Result<FileEntry, ProtocolError> {
    let mut m = r.map(depth)?;
    let (mut name, mut size, mut media_type, mut modified) = (None, None, None, None);
    while let Some(k) = m.next_key(r)? {
        match k {
            1 => {
                let n = r.text(MAX_WIRE_NAME_BYTES, "file name")?;
                validate_wire_name(n).map_err(|_| ProtocolError::Violation("invalid file name"))?;
                name = Some(n.to_owned());
            }
            2 => size = Some(r.uint_max(MAX_FILE_SIZE, "file size")?),
            3 => media_type = Some(r.text(MAX_MEDIA_TYPE, "media type")?.to_owned()),
            4 => modified = Some(r.int()?),
            _ => r.skip_value(m.value_depth())?,
        }
    }
    Ok(FileEntry {
        name: name.ok_or(missing())?,
        size: size.ok_or(missing())?,
        media_type,
        modified,
    })
}

fn decode_hello(r: &mut Reader<'_>, m: &mut MapReader, from: Role) -> Result<Hello, ProtocolError> {
    let mut device_name = None;
    let mut device_kind = DeviceKind::Unknown;
    let mut software = None;
    let mut features = Vec::new();
    let mut hash_algs = None;
    let mut intent = None;
    let mut sas_reveal = None;
    while let Some(k) = m.next_key(r)? {
        match k {
            1 => {
                let n = r.text(MAX_DEVICE_NAME_WIRE, "device name")?;
                if n.is_empty() {
                    return Err(ProtocolError::Violation("empty device name"));
                }
                device_name = Some(n.to_owned());
            }
            2 => device_kind = DeviceKind::from_code(r.uint()?),
            3 => software = Some(r.text(MAX_SOFTWARE, "software")?.to_owned()),
            4 => {
                let n = r.array_len(MAX_FEATURES as u64, "features")?;
                for _ in 0..n {
                    features.push(r.u16("feature id")?);
                }
            }
            5 => {
                let n = r.array_len(MAX_HASH_ALGS as u64, "hash algorithms")?;
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(r.u16("hash algorithm")?);
                }
                hash_algs = Some(v);
            }
            // A key not defined for the sender's role is an unknown key:
            // skipped, never interpreted.
            6 if from == Role::Initiator => {
                intent = Some(
                    Intent::from_code(r.uint()?)
                        .ok_or(ProtocolError::Violation("unknown intent"))?,
                );
            }
            7 if from == Role::Responder => sas_reveal = Some(r.bytes32()?),
            _ => r.skip_value(m.value_depth())?,
        }
    }
    let hash_algs = hash_algs.ok_or(missing())?;
    if !hash_algs.contains(&HASH_ALG_BLAKE3) {
        return Err(ProtocolError::Violation("peer does not support BLAKE3"));
    }
    match from {
        Role::Initiator if intent.is_none() => return Err(missing()),
        Role::Responder if sas_reveal.is_none() => return Err(missing()),
        _ => {}
    }
    Ok(Hello {
        device_name: device_name.ok_or(missing())?,
        device_kind,
        software,
        features,
        hash_algs,
        intent,
        sas_reveal,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> TransferId {
        TransferId::from_bytes([0xAB; 16])
    }

    fn roundtrip(msg: &Message, from: Role) {
        let mut buf = Vec::new();
        encode(msg, &mut buf).unwrap();
        assert_eq!(
            decode(&buf, from).unwrap(),
            Decoded::Message(msg.clone()),
            "{msg:?}"
        );
    }

    fn samples() -> Vec<(Message, Role)> {
        let hello_i = Hello {
            device_name: "Alice's Mac".into(),
            device_kind: DeviceKind::Laptop,
            software: Some("dropumi/0.1.0".into()),
            features: vec![],
            hash_algs: vec![1],
            intent: Some(Intent::Transfer),
            sas_reveal: None,
        };
        let hello_r = Hello {
            device_name: "Bob".into(),
            device_kind: DeviceKind::Unknown,
            software: None,
            features: vec![7, 9],
            hash_algs: vec![2, 1],
            intent: None,
            sas_reveal: Some([5; 32]),
        };
        vec![
            (Message::Hello(hello_i), Role::Initiator),
            (Message::Hello(hello_r), Role::Responder),
            (Message::PairResult { confirmed: true }, Role::Responder),
            (Message::error(ErrorCode::Busy), Role::Responder),
            (
                Message::Error {
                    code: ErrorCode::Unknown(77),
                    detail: None,
                },
                Role::Initiator,
            ),
            (
                Message::Cancel {
                    reason: CancelReason::Storage,
                },
                Role::Responder,
            ),
            (
                Message::Offer {
                    transfer_id: id(),
                    file_count: 2,
                    total_size: 12,
                },
                Role::Initiator,
            ),
            (
                Message::OfferFiles {
                    transfer_id: id(),
                    first_index: 0,
                    files: vec![
                        FileEntry {
                            name: "résumé ✓ 東京.txt".into(),
                            size: 12,
                            media_type: Some("text/plain".into()),
                            modified: Some(-5),
                        },
                        FileEntry {
                            name: "empty".into(),
                            size: 0,
                            media_type: None,
                            modified: None,
                        },
                    ],
                },
                Role::Initiator,
            ),
            (Message::Accept { transfer_id: id() }, Role::Responder),
            (
                Message::Reject {
                    transfer_id: id(),
                    reason: RejectReason::Declined,
                },
                Role::Responder,
            ),
            (
                Message::FileStart {
                    index: 3,
                    offset: 0,
                },
                Role::Initiator,
            ),
            (
                Message::FileEnd {
                    index: 3,
                    hash: FileHash::Blake3([9; 32]),
                },
                Role::Initiator,
            ),
            (
                Message::FileAbort {
                    index: 1,
                    reason: AbortReason::SourceChanged,
                },
                Role::Initiator,
            ),
            (
                Message::FileAck {
                    index: 1,
                    status: AckStatus::WriteFailed,
                },
                Role::Responder,
            ),
            (Message::Complete { transfer_id: id() }, Role::Initiator),
            (
                Message::CompleteAck {
                    transfer_id: id(),
                    stored_count: 2,
                },
                Role::Responder,
            ),
        ]
    }

    #[test]
    fn message_roundtrip_all_types() {
        for (m, from) in samples() {
            roundtrip(&m, from);
        }
    }

    #[test]
    fn wrong_direction_rejected() {
        let mut buf = Vec::new();
        encode(&Message::Accept { transfer_id: id() }, &mut buf).unwrap();
        assert_eq!(
            decode(&buf, Role::Initiator),
            Err(ProtocolError::Violation(
                "message sent in the wrong direction"
            ))
        );
    }

    #[test]
    fn ignores_unknown_keys() {
        // [18, {1: h'AB..', 2: [1, {}], 9: "x"}] — Accept with extra keys.
        let mut buf = vec![0x82, 0x12, 0xa3, 0x01, 0x50];
        buf.extend_from_slice(&[0xAB; 16]);
        buf.extend_from_slice(&[0x02, 0x82, 0x01, 0xa0, 0x09, 0x61, b'x']);
        assert_eq!(
            decode(&buf, Role::Responder).unwrap(),
            Decoded::Message(Message::Accept { transfer_id: id() })
        );
    }

    #[test]
    fn ignorable_type_skipped() {
        // [128, {1: 42}]
        let buf = [0x82, 0x18, 0x80, 0xa1, 0x01, 0x18, 0x2a];
        assert_eq!(
            decode(&buf, Role::Responder).unwrap(),
            Decoded::Ignored(128)
        );
        // Ignorable messages must still be strict CBOR.
        let bad = [0x82, 0x18, 0x80, 0xa1, 0x01, 0xf6];
        assert!(decode(&bad, Role::Responder).is_err());
    }

    #[test]
    fn unknown_required_type_unsupported() {
        let buf = [0x82, 0x05, 0xa0];
        assert_eq!(
            decode(&buf, Role::Initiator),
            Err(ProtocolError::Unsupported(5))
        );
    }

    #[test]
    fn rejects_bad_values() {
        let enc = |m: &Message| {
            let mut b = Vec::new();
            encode(m, &mut b).unwrap();
            b
        };
        // File names with separators or dot entries are violations.
        for bad in ["..", "a/b", "c\\d", ""] {
            let m = Message::OfferFiles {
                transfer_id: id(),
                first_index: 0,
                files: vec![FileEntry {
                    name: bad.into(),
                    size: 1,
                    media_type: None,
                    modified: None,
                }],
            };
            assert!(decode(&enc(&m), Role::Initiator).is_err(), "{bad:?}");
        }
        let long = Message::OfferFiles {
            transfer_id: id(),
            first_index: 0,
            files: vec![FileEntry {
                name: "x".repeat(1025),
                size: 1,
                media_type: None,
                modified: None,
            }],
        };
        assert_eq!(
            decode(&enc(&long), Role::Initiator),
            Err(ProtocolError::LimitExceeded("file name"))
        );
        let zero = Message::Offer {
            transfer_id: id(),
            file_count: 0,
            total_size: 0,
        };
        assert!(decode(&enc(&zero), Role::Initiator).is_err());
        let too_many = Message::Offer {
            transfer_id: id(),
            file_count: 10_001,
            total_size: 0,
        };
        assert!(decode(&enc(&too_many), Role::Initiator).is_err());
        // Hello from an initiator without intent, or without BLAKE3.
        let mut h = Hello {
            device_name: "x".into(),
            device_kind: DeviceKind::Unknown,
            software: None,
            features: vec![],
            hash_algs: vec![1],
            intent: None,
            sas_reveal: None,
        };
        assert!(decode(&enc(&Message::Hello(h.clone())), Role::Initiator).is_err());
        h.intent = Some(Intent::Pair);
        h.hash_algs = vec![2];
        assert!(decode(&enc(&Message::Hello(h.clone())), Role::Initiator).is_err());
        // Keys for the other role are skipped, not fatal.
        h.hash_algs = vec![1];
        h.sas_reveal = Some([1; 32]);
        match decode(&enc(&Message::Hello(h.clone())), Role::Initiator).unwrap() {
            Decoded::Message(Message::Hello(got)) => assert_eq!(got.sas_reveal, None),
            other => panic!("{other:?}"),
        }
        h.sas_reveal = Some([1; 32]);
        match decode(&enc(&Message::Hello(h)), Role::Responder).unwrap() {
            Decoded::Message(Message::Hello(got)) => assert_eq!(got.intent, None),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn wire_details_are_static_and_pathless() {
        for c in 0..=9u64 {
            let d = ErrorCode::from_code(c).wire_detail();
            assert!(d.is_ascii() && d.len() <= 64);
            assert!(!d.contains(['/', '\\', ':']));
        }
    }
}
