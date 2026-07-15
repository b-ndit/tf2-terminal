//! The guest-memory bookkeeping half of the ABI described in
//! `src-tauri/src/infra/plugins/runtime.rs`'s module doc comment. A plugin
//! must expose `alloc`/`dealloc`/`memory` at the top level of its compiled
//! module — [`crate::export_abi`] generates those exports directly in the
//! plugin's own crate (required so they survive linking; see that macro's
//! doc comment for why).

use std::alloc::{alloc, dealloc, Layout};

const ALIGN: usize = 8;

/// Allocates `len` bytes the host (or this SDK) can write into. Never
/// returns null for `len >= 0` in practice — an allocation failure inside
/// a plugin call is unrecoverable anyway (there is no "try again" for a
/// wasm guest OOM), so this matches `std`'s own default OOM-abort
/// behavior rather than adding a `Result` nobody could act on.
pub fn alloc_impl(len: i32) -> i32 {
    let layout = Layout::from_size_align(len.max(1) as usize, ALIGN).expect("valid layout");
    unsafe { alloc(layout) as i32 }
}

/// Frees a buffer previously returned by [`alloc_impl`]. `len` must match
/// the original allocation exactly (same convention `alloc`/`dealloc`
/// pairs use everywhere in this ABI).
pub fn dealloc_impl(ptr: i32, len: i32) {
    if ptr == 0 {
        return;
    }
    let layout = Layout::from_size_align(len.max(1) as usize, ALIGN).expect("valid layout");
    unsafe { dealloc(ptr as *mut u8, layout) }
}

/// Reads `len` bytes at `ptr` out of the plugin's own linear memory — used
/// to decode the `(in_ptr, in_len)` arguments the host passes into
/// `plugin_init`/`on_alert_fired`/`provide_listings`. Safe as long as the
/// host only ever calls those exports with a range it itself wrote (which
/// is the only way it does).
pub fn read_input(in_ptr: i32, in_len: i32) -> Vec<u8> {
    if in_len <= 0 {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(in_ptr as *const u8, in_len as usize).to_vec() }
}

/// Allocates a buffer, copies `bytes` into it, and packs `(ptr << 32) |
/// len` — the fixed return convention every host-called export in this
/// system uses. Returns `0` (packed "nothing") for empty input, matching
/// the host side's own `0` sentinel.
pub fn pack_output(bytes: &[u8]) -> i64 {
    if bytes.is_empty() {
        return 0;
    }
    let ptr = alloc_impl(bytes.len() as i32);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
    }
    ((ptr as i64) << 32) | (bytes.len() as i64)
}

/// Generates the `alloc`/`dealloc` exports a plugin's compiled module must
/// have, calling into this SDK's implementation. Invoke this **once**, at
/// the plugin crate's own root (not from within the SDK) — a `#[no_mangle]`
/// item defined only inside a dependency `rlib` and never referenced by
/// the plugin's own code is not guaranteed to survive into the final
/// linked module (an unreferenced item in a static library is fair game
/// for the linker to discard, `#[no_mangle]` only stops *renaming*, not
/// dead-code elimination); generating them directly in the plugin's own
/// crate sidesteps that entirely.
#[macro_export]
macro_rules! export_abi {
    () => {
        #[no_mangle]
        pub extern "C" fn alloc(len: i32) -> i32 {
            $crate::abi::alloc_impl(len)
        }

        #[no_mangle]
        pub extern "C" fn dealloc(ptr: i32, len: i32) {
            $crate::abi::dealloc_impl(ptr, len)
        }
    };
}
