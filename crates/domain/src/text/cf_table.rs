// SPDX-License-Identifier: MPL-2.0
//! Unicode General_Category=Cf (format characters), Unicode 17.0.0.
//!
//! Generated from the Unicode Character Database file
//! `https://www.unicode.org/Public/17.0.0/ucd/extracted/DerivedGeneralCategory.txt`
//! (sha256 `d62e5bab70ca74f099343f71224fa051cb1fdd61a1ab45c0488c44cfc0b6102e`)
//! by selecting every `Cf` entry and merging adjacent ranges:
//!
//! ```text
//! python3 - DerivedGeneralCategory.txt <<'EOF'
//! import sys
//! r = []
//! for l in open(sys.argv[1], encoding="utf-8"):
//!     l = l.split("#")[0].strip()
//!     if not l: continue
//!     cp, cat = [x.strip() for x in l.split(";")]
//!     if cat != "Cf": continue
//!     a, _, b = cp.partition("..")
//!     r.append((int(a, 16), int(b or a, 16)))
//! m = []
//! for a, b in sorted(r):
//!     if m and a == m[-1][1] + 1: m[-1] = (m[-1][0], b)
//!     else: m.append((a, b))
//! for a, b in m: print(f"    (0x{a:04X}, 0x{b:04X}),")
//! EOF
//! ```
//!
//! The Unicode version must match `unicode_normalization::UNICODE_VERSION`;
//! a unit test flags drift.

/// Sorted, disjoint, inclusive code point ranges with General_Category=Cf.
pub(crate) const CF_RANGES: &[(u32, u32)] = &[
    (0x00AD, 0x00AD),
    (0x0600, 0x0605),
    (0x061C, 0x061C),
    (0x06DD, 0x06DD),
    (0x070F, 0x070F),
    (0x0890, 0x0891),
    (0x08E2, 0x08E2),
    (0x180E, 0x180E),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0001, 0xE0001),
    (0xE0020, 0xE007F),
];

/// Unicode version the table was generated from.
pub(crate) const CF_TABLE_UNICODE_VERSION: (u8, u8, u8) = (17, 0, 0);
