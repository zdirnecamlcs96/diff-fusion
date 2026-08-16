---
layout: default
title: SDKs
nav_order: 5
has_children: true
---

# SDKs

One Rust kernel, three delivery packages. The kernel — three-way diff, canonical JSON, idempotency keys, and built-in policy resolution — is Rust code; every other language delivers it compiled to WASM rather than re-implementing the logic. That's what makes the golden vectors in [the cross-language spec]({{ site.baseurl }}/reference/spec) meaningful: all three runtimes are asserted to agree bit-for-bit across all four kernel functions — three-way diff, merge-field resolution, canonical JSON, and idempotency keys — not just "close enough".

| | [Rust]({{ site.baseurl }}/sdks/rust) | [TypeScript]({{ site.baseurl }}/sdks/typescript) | [Go]({{ site.baseurl }}/sdks/go) |
|---|---|---|---|
| What you get | Full library: kernel + orchestrator, policies, ports, adapters, drivers | Full library, same layers as Rust — kernel delivered via wasm-bindgen | Kernel only — 4 functions, JSON bytes in/out |
| Kernel delivery | Native (it *is* the kernel) | WASM, vendored in `sdk/typescript/wasm/` | WASM (`wasm32-wasip1`), run by wazero |
| App layer (orchestrator, adapters, ports) | Native Rust | Native TypeScript | Not shipped — build it natively per host |
| Install | git dependency (not on crates.io yet) | `npm install github:zdirnecamlcs96/diff-fusion` (not on npm yet) | `go get github.com/zdirnecamlcs96/diff-fusion/sdk/golang` |

Rust is the source of truth: the kernel is written once there, and the TypeScript and Go deliveries consume the compiled artifact rather than a hand-maintained port. Where a language needs the full reconciliation pipeline (policies, ancestor store, escalation queue), that layer is native to the language — TypeScript ships one, Go doesn't yet, so Go callers write their own orchestration around the four kernel primitives.

Pick a page for install instructions and the full method/export list:

- [Rust]({{ site.baseurl }}/sdks/rust)
- [TypeScript]({{ site.baseurl }}/sdks/typescript)
- [Go]({{ site.baseurl }}/sdks/go)
