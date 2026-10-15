// SPDX-License-Identifier: MPL-2.0
//! Protocol v1 constants and hard limits.

/// Maximum preamble size: magic (4) + format (1) + count (1) + 8 × u16.
pub const PREAMBLE_MAX: usize = 22;
/// Maximum number of versions in a preamble.
pub const MAX_VERSIONS: usize = 8;
/// Exact size of Noise message 1 (`-> e`, empty payload).
pub const MSG1_LEN: usize = 32;
/// Maximum size of a handshake message.
pub const MAX_HANDSHAKE_MSG: usize = 512;
/// Maximum size of a handshake payload.
pub const MAX_HANDSHAKE_PAYLOAD: usize = 256;
/// Noise maximum message size.
pub const NOISE_MAX_MSG: usize = 65_535;
/// AEAD tag length.
pub const TAG_LEN: usize = 16;
/// Smallest transport message (one plaintext byte plus tag).
pub const MIN_TRANSPORT_MSG: usize = 1 + TAG_LEN;
/// Maximum plaintext per transport message.
pub const MAX_PLAINTEXT: usize = NOISE_MAX_MSG - TAG_LEN;
/// Maximum frame body (plaintext minus the frame type byte).
pub const MAX_FRAME_BODY: usize = MAX_PLAINTEXT - 1;
/// Maximum total encoded size of all `OfferFiles` metadata in one offer.
pub const MAX_OFFER_METADATA: usize = 4 * 1024 * 1024;
/// Maximum device name length on the wire.
pub const MAX_DEVICE_NAME_WIRE: usize = 128;
/// Maximum software string length.
pub const MAX_SOFTWARE: usize = 64;
/// Maximum media type length.
pub const MAX_MEDIA_TYPE: usize = 127;
/// Maximum error detail length.
pub const MAX_ERROR_DETAIL: usize = 256;
/// Maximum number of feature ids.
pub const MAX_FEATURES: usize = 32;
/// Maximum number of hash algorithm ids.
pub const MAX_HASH_ALGS: usize = 8;
/// Maximum entries in one CBOR map.
pub const MAX_MAP_ENTRIES: usize = 32;
/// Maximum CBOR nesting depth of a message.
pub const MAX_DEPTH: usize = 4;
/// Maximum consecutive ignorable messages.
pub const MAX_CONSECUTIVE_IGNORABLE: usize = 16;
/// Protocol versions this implementation supports, highest first.
pub const SUPPORTED_VERSIONS: &[u16] = &[1];
/// Noise protocol name for version 1.
pub const NOISE_PARAMS: &str = "Noise_XX_25519_ChaChaPoly_SHA256";
/// Hash algorithm id for BLAKE3-256.
pub const HASH_ALG_BLAKE3: u16 = 1;
/// Length of a BLAKE3-256 digest.
pub const BLAKE3_LEN: usize = 32;
