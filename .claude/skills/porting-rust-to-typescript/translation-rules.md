# Translation rules — full reference

Each row: the Rust idiom, the TypeScript pattern, and a real example from the diff-fusion port. Examples use `src/...` for the Rust source and `ts/src/...` for the TS port — both relative to the diff-fusion repo root.

## Errors and control flow

### `Result<T, E>` — split by call site

**Throw** for adapter / IO failures. The orchestrator and CLI catch by `kind`, so a thrown error is just an out-of-band branch.

**Return** a discriminated union for in-domain results that a caller is *expected* to branch on (resolver outcomes, invariant verdicts).

| Direction | Rust | TS |
|---|---|---|
| Throw | `fn open(root: &Path) -> Result<Self, io::Error>` | `static async open(root: string): Promise<FsAncestor>` — throws `wrapIo(e)` (see `ts/src/adapters/filesystemAncestor.ts`, around the `open` static) |
| Return union | `fn merge(...) -> MergeOutcome` where `MergeOutcome` is an `enum` with `Resolved`/`Conflict` | `merge(...): MergeOutcome` where `MergeOutcome = { kind: "Resolved"; value } \| { kind: "Conflict"; reason }` — see `ts/src/application/policy/index.ts` |

Why not `neverthrow` / a Result library? Adds noise without earning much in a hexagonal codebase: domain functions already return discriminated unions, and IO functions only have one error mode (the thrown one). Keep the surface area small.

### Error enums specifically — class extending `Error`, not bare union

Rust enums deriving `thiserror::Error` are *both* thrown and pattern-matched. A bare TS discriminated union loses stack traces and `instanceof` — both of which JS code relies on. The diff-fusion convention is `class extends Error` carrying a `kind` discriminator field (and any variant payload), with static constructors that mirror Rust's `impl SyncError { fn transient(...) -> Self }`.

Variant tag casing: **preserve the Rust variant name (PascalCase)** by default. Only match a renamed JSON form when the Rust enum has `#[serde(rename_all = "snake_case")]` (or similar). The TS source `kind: "Transient"` lines up with `SyncError::Transient` in stack traces and logs.

### `enum` with payloads → discriminated union

For non-error enums (results, outcomes, classifications), use a plain discriminated union. Rust source — `src/domain/error.rs:14-34`:

```rust
#[derive(Debug, Error)]
pub enum SyncError {
    #[error("transient failure: {0}")]
    Transient(String),
    #[error("stale write: ...")]
    StaleWrite { system: String, expected: Option<String>, message: String },
    #[error("unresolved conflict(s): {paths:?}")]
    Conflict { paths: Vec<String> },
}
```

TS port — `ts/src/domain/error.ts:15-32`:

```ts
export type SyncErrorKind = "Transient" | "StaleWrite" | "Conflict";

interface TransientFields { kind: "Transient"; }
interface StaleWriteFields { kind: "StaleWrite"; system: string; expected: string | undefined; }
interface ConflictFields  { kind: "Conflict"; paths: readonly string[]; }
```

Note: `SyncError` is a *class* in TS so it carries a stack trace; the discriminated union `Fields` carries the payload shape. Static constructors (`SyncError.transient(...)`, `SyncError.staleWrite({...})`) mirror Rust's `impl SyncError { fn transient(...) -> Self }`.

### Exhaustive `match` → `switch` with `never` check

```ts
switch (outcome.kind) {
  case "Resolved": return outcome.value;
  case "Conflict": return ctx.escalate(outcome.reason);
  default: {
    const _: never = outcome;
    throw new Error(`unhandled MergeOutcome: ${(_ as MergeOutcome).kind}`);
  }
}
```

The `const _: never = outcome` line fails compilation the moment you add a new union variant without updating the switch.

## Types

### `Option<T>` → `T | undefined`

Always `undefined`, never `null`. Adapter wire formats (JSON sent to external systems) treat the field as **omitted** when the Option is `None`.

`tsconfig.json` must set `"exactOptionalPropertyTypes": true`. With that flag, `{ field?: string }` accepts an omitted field but rejects `{ field: undefined }`. Construct without setting the key when you want it absent:

```ts
const fields: StaleWriteFields = { kind: "StaleWrite", system: "sap" };
if (expected !== undefined) (fields as any).expected = expected;
// or, more cleanly:
const fields = expected === undefined
  ? { kind: "StaleWrite", system: "sap" }
  : { kind: "StaleWrite", system: "sap", expected };
```

### `BTreeMap<String, _>` → sort before serialising

JS objects iterate in *insertion order* for string keys. Rust `BTreeMap` iterates in lexicographic order. Anything that hashes or serialises a JSON value cross-runtime must sort recursively first.

The reusable helper lives in `ts/src/domain/idempotency.ts` (see `canonicalStringify`). Use it; do not inline ad-hoc key sorting elsewhere.

### `HashMap<K, V>` → `Map<K, V>` (not POJO)

Use `Map` whenever keys aren't fixed string literals. Reasons: real key types (numbers, tuples), order preservation, missing-key semantics (`map.get(k)` returns `undefined` cleanly without prototype-pollution risk).

POJO is fine for fixed string-keyed records like `presets`, schema declarations, etc.

### `Vec<u8>` → `Uint8Array`

Always. `Buffer` is Node-only; `Uint8Array` works in browsers and is what `@noble/hashes` consumes.

### `u64` ms timestamp → `number`

JS `number` covers integers up to `2^53 − 1` exactly. That's safe for ms-since-epoch through year 5138. Don't use `BigInt` for timestamps — it forces consumers to tag every arithmetic op and breaks `JSON.stringify`.

### `&str` / `String` → `string`

No distinction. Borrow vs own is a Rust ownership concept; in TS strings are immutable values.

## Traits and dispatch

### `trait` + `impl` → `interface` + class

Stateless policies in diff-fusion (OwnedBy, Additive, Append, StateMachine) are *classes implementing an interface*, not factory functions, even though they have no state. Rationale: Rust readers expect `impl Trait for Struct` to map to a named class; debugging printouts include the class name.

Rust — `src/application/policy/owned_by.rs`:

```rust
pub struct OwnedBy { pub system: String }
impl MergePolicy for OwnedBy { fn merge(...) -> MergeOutcome { ... } }
```

TS — `ts/src/application/policy/ownedBy.ts:20-50`:

```ts
export class OwnedBy implements MergePolicy {
  readonly system: string;
  constructor(system: string) { this.system = system; }
  name(): string { return "owned_by"; }
  merge(change: FieldChange, ctx: MergeContext): MergeOutcome { ... }
}
```

### `Box<dyn T>` / `Arc<dyn T>` → just the interface

Rust uses `Arc<dyn AncestorStore>` to share a trait object across threads. JS has a single shared heap and a GC. The TS field type is just the interface:

```ts
class Orchestrator {
  constructor(private readonly ancestors: AncestorStore, ...) {}
}
```

No ref-counting, no `Mutex`. If you genuinely need shared-mutable cross-worker state in Node, that's an `AsyncLocalStorage` or a worker-threads design — not an Arc port.

### `#[async_trait]` → native `async` on interface

```ts
export interface SystemPort {
  pull(entityType: string): Promise<readonly EntitySnapshot[]>;
  push(canonicalId: string, op: string, payload: JsonValue): Promise<void>;
}
```

No marker decoration needed.

## Serde

### `#[serde(tag = "...", rename_all = "...")]` → match the JSON shape exactly

Rust — `src/application/policy/declaration.rs` (the `MergePolicyRef` enum):

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MergePolicyRef {
    OwnedBy { system: String },
    Additive,
    Append,
    StateMachine { transitions: Vec<StateTransition> },
}
```

TS — `ts/src/application/policy/declaration.ts`:

```ts
export type MergePolicyRef =
  | { kind: "owned_by"; system: string }
  | { kind: "additive" }
  | { kind: "append" }
  | { kind: "state_machine"; transitions: TransitionRef[] };
```

The Rust variant `OwnedBy` becomes JSON tag `"owned_by"` because of `rename_all = "snake_case"`. The TS literal must match that JSON, **not** the Rust source name. If the JSON shape is shared (declarations stored in a database, sent across the wire, used as configuration), getting this wrong silently breaks interchange.

### `#[serde(skip_serializing_if = "Option::is_none")]` → omit, don't emit `null`

Same rule as `Option<T>` above. Build the object without the key.

### Field renames (`#[serde(rename = "foo_bar")]`)

The TS field name **is** the renamed name. If Rust has `#[serde(rename = "case_id")] case_id: String`, the TS interface is `{ case_id: string }`, not `{ caseId: string }` with a serialiser layer. You're paying camelCase conversion to nobody — the wire is the only thing that matters, and it stayed the same in Rust.

If you really want camelCase in TS code, write a one-shot canonicalising layer at the adapter boundary. For diff-fusion the call was: don't bother; consistency with Rust JSON is more valuable than house-style camelCase.

## Test structure

### `#[test]` → `it()` with the same name

Always port the Rust test names verbatim, snake_case and all:

```ts
describe("idempotencyKey — ported Rust tests", () => {
  it("deterministic_for_same_inputs", () => { ... });
  it("key_order_in_payload_does_not_matter", () => { ... });
});
```

When a future change touches the Rust file, the diff at the test level is line-for-line comparable. Don't "improve" by translating to "should be deterministic for same inputs" — you lose that property.

### `proptest!` → `fast-check`

Hand-translate the strategy. Rust `prop::num::i64::ANY` → `fc.integer({ min: -(2**53 - 1), max: 2**53 - 1 })` (clamped to safe range; see gotchas). Rust `"[A-Z0-9-]{1,16}"` → `fc.stringMatching(/^[A-Z0-9-]{1,16}$/)` or compose from `fc.constantFrom`.

### Cross-runtime fixtures — Rust generates, TS verifies

Pattern:

1. `examples/gen_idempotency_vectors.rs` — Rust binary that emits a JSON array of `{ canonicalId, operation, payload, canonicalPayloadJson, expectedHex }` to stdout.
2. Run once and commit: `cargo run --example gen_idempotency_vectors > ts/tests/fixtures/idempotency-vectors.json`.
3. TS test loads the fixture, recomputes `idempotencyKeyHex(canonicalId, operation, payload)` for each row, asserts equality with `expectedHex`.

Rust is the source of truth. TS verifies. CI runs the TS test; the Rust generator is run manually whenever new vectors are added.

## NodeNext-specific

### `.js` import extensions

```ts
// In TS source. The .js is what NodeNext resolves at runtime.
import { idempotencyKey } from "./idempotency.js";
import type { JsonValue } from "../domain/types.js";
```

Don't use bare specifiers (`./idempotency`) — they error at runtime. Don't use `.ts` — they error at type-check time.

### Two tsconfigs

- `tsconfig.json` — `src/` only, emits to `dist/`.
- `tsconfig.test.json` — extends the first, includes `tests/`, sets `noEmit`. Run with `npx tsc --noEmit -p tsconfig.test.json` to type-check tests separately.

### Project-local npm cache

If `~/.npm` is root-owned (common after sudo accidents), use `npm install --cache "$(pwd)/.npm-cache"` to keep installs working without `sudo`.
