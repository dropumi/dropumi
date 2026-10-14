// SPDX-License-Identifier: MPL-2.0
//! UniFFI's generator (Kotlin, and Swift sources), pinned to the workspace's
//! UniFFI version so generated code always matches the library.

fn main() {
    uniffi::uniffi_bindgen_main();
}
