//! WASI p1 kernel driver — C ABI over the shared wire layer, for non-JS
//! hosts (Go via wazero). See docs/superpowers/specs/2026-07-26-p4-go-delivery-design.md.
//!
//! ABI: each string argument is a (ptr, len) u32 pair of UTF-8 bytes the
//! host wrote into guest memory allocated with `df_alloc`. Each function
//! returns a packed u64 `(ptr << 32) | len` for a guest-allocated UTF-8
//! JSON envelope: `{"ok":"<result>"}` or `{"err":"<message>"}`. The host
//! copies the envelope out, then frees it with `df_dealloc`. Argument
//! buffers are borrowed by the guest and freed by the host. Every argument
//! pointer must be non-null: hosts allocate via `df_alloc` even for empty
//! strings (`df_alloc(0)` returns a dangling-but-valid pointer, and
//! `df_dealloc(ptr, 0)` on it is a no-op).

use super::wire::{
    canonical_json_impl, compare_json_impl, idempotency_key_hex_impl, merge_field_impl,
    three_way_diff_impl, transform_to_cif_impl,
};

#[unsafe(no_mangle)]
pub extern "C" fn df_alloc(len: u32) -> u32 {
    let buf = vec![0u8; len as usize].into_boxed_slice();
    Box::into_raw(buf) as *mut u8 as u32
}

/// Frees a buffer previously returned by `df_alloc` or a result envelope.
#[unsafe(no_mangle)]
pub extern "C" fn df_dealloc(ptr: u32, len: u32) {
    unsafe {
        drop(Box::from_raw(std::slice::from_raw_parts_mut(
            ptr as *mut u8,
            len as usize,
        )));
    }
}

// The returned `&str` borrows guest memory owned by the host; it is valid
// only for the duration of the current export call (the unbounded `'a`
// cannot enforce this).
fn arg<'a>(ptr: u32, len: u32) -> Result<&'a str, String> {
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    std::str::from_utf8(bytes).map_err(|e| format!("invalid UTF-8 argument: {e}"))
}

fn envelope(res: Result<String, String>) -> u64 {
    let json = match res {
        Ok(v) => serde_json::json!({ "ok": v }),
        Err(e) => serde_json::json!({ "err": e }),
    };
    let buf = json.to_string().into_bytes().into_boxed_slice();
    let len = buf.len() as u32;
    let ptr = Box::into_raw(buf) as *mut u8 as u32;
    ((ptr as u64) << 32) | len as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn df_three_way_diff(
    anc_ptr: u32,
    anc_len: u32,
    a_ptr: u32,
    a_len: u32,
    b_ptr: u32,
    b_len: u32,
) -> u64 {
    envelope((|| {
        three_way_diff_impl(arg(anc_ptr, anc_len)?, arg(a_ptr, a_len)?, arg(b_ptr, b_len)?)
    })())
}

#[unsafe(no_mangle)]
pub extern "C" fn df_merge_field(
    change_ptr: u32,
    change_len: u32,
    policy_ptr: u32,
    policy_len: u32,
    ctx_ptr: u32,
    ctx_len: u32,
) -> u64 {
    envelope((|| {
        merge_field_impl(
            arg(change_ptr, change_len)?,
            arg(policy_ptr, policy_len)?,
            arg(ctx_ptr, ctx_len)?,
        )
    })())
}

#[unsafe(no_mangle)]
pub extern "C" fn df_canonical_json(doc_ptr: u32, doc_len: u32) -> u64 {
    envelope((|| canonical_json_impl(arg(doc_ptr, doc_len)?))())
}

#[unsafe(no_mangle)]
pub extern "C" fn df_compare_json(a_ptr: u32, a_len: u32, b_ptr: u32, b_len: u32) -> u64 {
    envelope((|| compare_json_impl(arg(a_ptr, a_len)?, arg(b_ptr, b_len)?))())
}

#[unsafe(no_mangle)]
pub extern "C" fn df_transform_to_cif(
    source_ptr: u32,
    source_len: u32,
    schema_ptr: u32,
    schema_len: u32,
    format_id_ptr: u32,
    format_id_len: u32,
) -> u64 {
    envelope((|| {
        transform_to_cif_impl(
            arg(source_ptr, source_len)?,
            arg(schema_ptr, schema_len)?,
            arg(format_id_ptr, format_id_len)?,
        )
    })())
}

#[unsafe(no_mangle)]
pub extern "C" fn df_idempotency_key_hex(
    id_ptr: u32,
    id_len: u32,
    op_ptr: u32,
    op_len: u32,
    payload_ptr: u32,
    payload_len: u32,
) -> u64 {
    envelope((|| {
        idempotency_key_hex_impl(
            arg(id_ptr, id_len)?,
            arg(op_ptr, op_len)?,
            arg(payload_ptr, payload_len)?,
        )
    })())
}
