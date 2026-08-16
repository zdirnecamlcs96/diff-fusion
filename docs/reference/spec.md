---
layout: default
title: Cross-language spec
parent: Reference
nav_order: 4
---

# Cross-language spec

`spec/` at the repo root is the contract between the three deliveries. Rust is the kernel's source of truth ([SDKs]({{ site.baseurl }}/sdks)); TypeScript and Go each compile the same Rust code to WASM rather than re-implementing it. `spec/` is how that claim is checked rather than just asserted: JSON Schemas pin the wire shapes crossing each language boundary, and golden vectors pin the actual bytes each kernel operation must produce.

## JSON Schemas (`spec/schema/`)

Three schemas, each generated from a real Rust type:

- **`wire-changelog.schema.json`** (`WireChangelog`) — the wire shape of a three-way diff result: the boundary format `three_way_diff` actually returns. It preserves the absent-key-vs-`null` distinction (an absent key means a side didn't touch the field; a present `null` means it was cleared) — collapsing that distinction loses information no consumer can recover.
- **`policy-config.schema.json`** (`MergePolicyRef`) — the serializable declaration of a per-field merge policy, so a policy set can be expressed as JSON (config file, API payload) instead of only as Rust/TS code.
- **`merge-outcome.schema.json`** (`MergeOutcomeWire`) — mirrors the ad-hoc `{"kind":"Resolved","value":...}` / `{"kind":"Conflict","reason":...}` JSON that `drivers::wire::merge_field_impl` builds inline. No runtime type carries this shape in Rust — it's assembled with `serde_json::json!` — so this schema-only type stands in for it as documentation of what crosses the wire.

## Golden vectors (`spec/vectors/`)

- **`idempotency-vectors.json`** — 82 vectors, each `{ canonicalId, operation, payload, canonicalPayloadJson, expectedHex }`. `canonicalPayloadJson` is the exact byte string `serde_json::to_string` produced for that payload; `expectedHex` is the BLAKE3 digest computed from it. TS and Go tests read this file and assert their own idempotency-key computation matches `expectedHex` byte-for-byte.
- **`filesystem-filenames.json`** — 12 vectors, each `{ entityType, canonicalId, expectedEntityDir, expectedFile, expectedRelPath }`, pinning the `<sanitized_entity>/<blake3_hash>.json` path the filesystem ancestor store derives for a given key. Other-language filesystem ancestor adapters assert byte-identical path derivation against these.

Both files are generated **only** by the Rust examples — there is no TS or Go generator. Rust is authoritative; the other two runtimes read the file and must match it, never regenerate their own copy.

## Regenerating the vectors and schemas

From `src/`:

```bash
cargo run --example gen_schema --features schema-gen
cargo run --example gen_idempotency_vectors > ../spec/vectors/idempotency-vectors.json
cargo run --example gen_filesystem_filenames > ../spec/vectors/filesystem-filenames.json
```

`gen_schema` writes all three schema files directly into `spec/schema/` (it resolves the path from `CARGO_MANIFEST_DIR`, so it's safe to run from any working directory as long as the command itself runs `cargo run` from `src/`). The vector generators print to stdout — the `>` redirect is what actually updates the checked-in file.

## Per-language conformance

Each delivery's own test suite is what checks its output against these vectors — there's no separate cross-language runner:

```bash
cargo test               # from src/          — Rust: also the vectors' origin
npm test                 # from sdk/typescript/ — TypeScript
go test ./...            # from sdk/golang/     — Go
```

If you change kernel behaviour in Rust, regenerate the vectors and schemas first, then run all three conformance commands — a diff in any of them means TS or Go drifted from the new Rust output.
