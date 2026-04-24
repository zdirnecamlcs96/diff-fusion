# diff-fusion playground

Interactive visualizer for `diff-fusion` two-way reconciliation. Define a
policy, paste data from two systems, click **Run Sync**, watch the pipeline
animate stage by stage.

The library in the parent crate is not modified — this playground consumes it
as a path dependency.

---

## Running

```bash
cargo run --manifest-path playground/Cargo.toml
```

Then open `http://127.0.0.1:3000/` in a browser. The page loads with a working
sample so you can click **Run Sync ▶** immediately.

---

## Architecture

```
diff-fusion/                    ← library crate, unchanged
└── playground/                 ← this crate (separate Cargo package)
    ├── Cargo.toml              # bin crate; diff_fusion = { path = ".." }
    ├── src/
    │   ├── main.rs             # Axum server, POST /sync, static file serving
    │   ├── dto.rs              # Serde-serializable mirrors of library types
    │   ├── pipeline.rs         # Orchestrates the 5 stages; parses policy JSON
    │   └── policies.rs         # Playground-local SetByKeyComposite policy
    └── web/                    # Static frontend, no build step
        ├── index.html          # Single page — inputs + pipeline + outcome
        ├── style.css
        └── app.js              # Vanilla JS: fetch /sync + animated pipeline
```

**Why a separate crate.** The diff-fusion library exposes `SyncOutcome`,
`FacadeConflict`, etc. but they don't derive `Serialize` yet. We mirror them
in `dto.rs` and convert at the HTTP boundary instead of modifying the library.

**Playground-local policy.** `SetByKeyComposite` (in `src/policies.rs`) extends
the library's `SetByKey` with composite identity, stable anchors, and
field-union merging. It composes the library's public `MergePolicy` trait — no
library modifications.

---

## The five inputs

| Field | What it is | Required |
|---|---|---|
| **System A** | Raw JSON from the first system (whatever shape it uses) | yes |
| **System B** | Raw JSON from the second system (different shape is fine) | yes |
| **Schema** | `cif_schema` (target fields) + `transformations.<format>` (how to map each system's JSON into the CIF) | yes |
| **Policy** | Per-field merge rules — see [Policy reference](#policy-reference) | yes |
| **Ancestor** | The last-synced canonical state. Leave blank for **first sync** | no |
| **System A name / B name** | Labels used to look up transformations in the schema and to name the systems in `owned_by` policies | yes |

**Important:** the key you use for **System A name** must match both
`schema.transformations.<name>` and any `{"kind":"owned_by","system":"<name>"}`
in the policy. Same for B.

---

## Policy reference

Policy JSON has a single top-level key `per_field`. Each entry maps a **CIF
field path** to a policy declaration.

```json
{
  "per_field": {
    "price":     { "kind": "owned_by", "system": "erp" },
    "qty_recv":  { "kind": "additive" },
    "tags":      { "kind": "append" },
    "po_status": {
      "kind": "state_machine",
      "transitions": [
        { "from": "open", "to": "closed" },
        { "from": "open", "to": "cancelled" }
      ]
    }
  }
}
```

Supported `kind` values:

| Kind | What it does | Required fields |
|---|---|---|
| `owned_by` | One system wins; the other side's change is reverted | `system` (must match adapter name) |
| `additive` | Numeric counters — both sides' deltas relative to ancestor accumulate | — |
| `append` | Arrays — concatenate both sides' additions (no dedupe) | — |
| `state_machine` | Enum fields — only listed `(from, to)` transitions are accepted; anything else escalates | `transitions` |
| `set_by_key` | Arrays of objects — elements matched cross-system by `identity` (single field or **composite** list); optional stable anchors re-home mutated rows | `identity` (string OR array); optional `a_anchor` / `b_anchor` (each side's stable local PK); optional `on_both_changed` (`union` (default), `prefer_a`, `prefer_b`, `escalate`) |

Fields not listed in `per_field` that changed on both sides will produce a
`NoPolicy` conflict.

### Array merges: the anchor rule

**When you merge an array, the policy MUST name at least one unique
anchor per row.** Without an anchor the engine has no way to know which
A-row is which B-row, so it either duplicates (`append`) or escalates.

An anchor can take either of two shapes — most real reconciliations need
both:

1. **Business identity** (`identity`, required) — a field or tuple of
   fields that both systems populate and that uniquely names a row:
   `sku`, or `(sku, uom)`, or `(sku, uom, lot)`, etc. This is what tells
   the engine "A-row X is B-row Y."
2. **Per-system stable anchors** (`a_anchor` / `b_anchor`, optional) —
   each side's own local primary key (`externalId` on A, `internalId`
   on B). Required whenever an identity field can mutate. The ancestor
   stores both anchors, so the policy re-homes incoming rows to their
   ancestor row via the anchor *before* composite-identity matching
   runs.

Which anchors you need to declare:

| Case | `identity` | `a_anchor` / `b_anchor` |
|---|---|---|
| Business-identity fields never mutate | required | optional |
| Identity fields may mutate (rename `uom`, split a line, etc.) | required (for new rows / first sync) | **required** |
| First sync — no anchors in ancestor yet | required | no effect (ignored gracefully) |

### Cross-system items with per-system local IDs

When System A (external) uses `externalId` and System B (internal) uses
`internalId`, you cannot merge by local ID. Declare a canonical identity
that both systems populate — typically a business key like `sku`. But
single-field identity often isn't enough: the same `sku` may appear on
multiple lines with different unit-of-measure, lot, or warehouse. Use a
**composite identity** so `(SKU-100, BTL)` and `(SKU-100, CTN)` stay as
distinct lines:

```json
"items": {
  "kind": "set_by_key",
  "identity": ["sku", "uom"],
  "a_anchor": "externalId",
  "b_anchor": "internalId",
  "on_both_changed": "union"
}
```

But **identity fields can mutate**. If A renames a row's `uom` from
`CTN` to `BOX`, pure composite-key matching would see the old key
disappear and a new one appear — the per-system IDs would break apart.
The `a_anchor` / `b_anchor` fields are each side's stable local primary
key (the ID that never changes across syncs). The ancestor stores both,
so the policy re-homes each mutated row to its original ancestor row
via the anchor *before* composite matching runs.

With `on_both_changed: "union"`, matched lines get a shallow field
union — `externalId` (from A) and `internalId` (from B) both end up on
one merged record, linked via the anchor pair:

```json
{ "sku": "SKU-100", "uom": "BOX", "qty": 2,
  "externalId": "A-L2", "internalId": "B-I2" }
```

The seeded sample does exactly this: A renames the CTN line to BOX;
the anchor re-homing keeps it linked to B's CTN line (whose internalId
still matches the ancestor's). Swap `on_both_changed` to `prefer_a` to
see B's `internalId` drop out on conflict, or to `escalate` to force
the conflict path. Delete `a_anchor` / `b_anchor` entirely to see the
breakdown: A's renamed row appears as a brand-new line with only A's
ID, and B's original CTN line re-appears via removal-in-A detection.

---

## The pipeline stages

Visually the pipeline is four steps. Step 1 runs the two transforms in
parallel (they're independent) and they animate together in a single
"parallel" block:

1. **Transform A → CIF** + **Transform B → CIF** — apply
   `schema.transformations.<system_a_name>` and `<system_b_name>` to the
   respective raw inputs. Implemented with `tokio::spawn_blocking` +
   `tokio::join!` so they run truly in parallel on a blocking thread pool.
2. **3-way Diff** — three pairwise diffs: `A vs Ancestor`, `B vs Ancestor`,
   `A vs B`. Missing ancestor is treated as `{}` (first-sync path).
3. **Policy Resolution** — dry-run through every field that changed, applying
   your per-field policies. Emits `would_write` and any conflicts that would
   escalate.
4. **Outcome** — the real sync. Result is one of:
   - **Synced** — resolution was clean, both sides got written (`pushed_to`).
   - **Escalated** — at least one conflict survived; nothing was written.
   - **NoOp** — neither side changed since the ancestor.

Stages animate sequentially. Each card expands on activation to reveal its
intermediate data.

---

## Outcome detail

Below the pipeline, successful syncs (`Synced`) expose two sub-panels:

- **Merged CIF (would write)** — the canonical record the engine wrote back
  to both systems.
- **Field changelog** — a table showing one row per field path that
  actually changed. Columns:
  - **Path** — the CIF field name (e.g. `price`, `items`, `po_status`).
  - **Ancestor (from)** — starting value. Rendered with **git-style line
    diff**; fields present in the ancestor but missing / changed in the
    final value are shown as `- removed` (red).
  - **System A / System B** — each side's value post-transform, rendered as
    a line diff vs the ancestor so additions (`+` green) and unchanged
    context lines are visually distinct. The cell is highlighted when that
    side's value is the one that ended up winning for that field.
  - **Written (to)** — the final resolved value, same line-diff treatment.
  - **Winner** — which side (or policy) decided the outcome:
    - `<system_a_name>` / `<system_b_name>` — one side's raw value won
      (`owned_by`, or simple one-sided change).
    - `both` — both sides agreed.
    - `no-op` — matched ancestor.
    - `derived` — value came from a policy computation, not either side
      directly. For numeric additive fields, a small hint below the label
      shows the arithmetic, e.g. `5 + 1 + 2` for
      `ancestor + (A-delta) + (B-delta) = 8`.

**Escalated** outcomes show a conflict list (path / reason / class)
color-coded by `ConflictClass` (`NoPolicy` gray, `PolicyConflict` amber,
`InvariantViolation` red).

---

## Recipes

### See a conflict
Change `state_machine.transitions` in the policy to only allow `draft → open`.
The seeded data has both sides at `status = "closed"` coming from `"open"` in
the ancestor, so the policy will reject it — outcome becomes **Escalated** with
a `PolicyConflict` on `po_status`.

### First-sync path
Clear the **Ancestor** field entirely and re-run. No 3-way diff baseline →
orchestrator compares against `{}`.

### Trigger a `NoPolicy` conflict
Remove `"price"` from `per_field`. Both sides changed `price` → no policy →
`NoPolicy` conflict escalates.

### Swap authoritative system
Change `owned_by.system` from `"erp"` to `"inv"` (or rename the systems in the
**System A/B name** inputs). Re-run — the chosen owner's value now wins for
that field.

### See the anchor rule break
Delete `a_anchor` and `b_anchor` from the `items` policy, keep A's `uom`
rename. The renamed line will no longer match its ancestor row via anchor,
so composite-key matching sees it as "B's CTN removed" + "A's BOX added" —
the cross-system ID pairing breaks.

### Swap `on_both_changed` modes
On the items policy, cycle through `union` → `prefer_a` → `prefer_b` →
`escalate` and watch the Winner column + Merged CIF change:
`union` keeps both IDs; `prefer_a`/`prefer_b` drops the other side's
local ID; `escalate` produces a `PolicyConflict` in the outcome.

---

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| `Transform System A failed: Format 'X' not found in schema` | **System A name** doesn't match any key in `schema.transformations` |
| `Invalid policy for 'X': unknown variant ...` | Typo in `kind` (must be one of `owned_by`, `additive`, `append`, `state_machine`, `set_by_key`) |
| `Invalid policy for 'X': set_by_key requires identity` | `set_by_key` declaration is missing the `identity` field |
| `Invalid policy for 'X': unknown on_both_changed: ...` | Must be `union`, `prefer_a`, `prefer_b`, or `escalate` |
| `element missing identity field 'X'` | A row in the array doesn't carry every field listed in the composite `identity`. Usually means an ancestor row predates the composite key — add the missing field to the ancestor or simplify the identity. |
| `Sync failed: ...` | Usually a schema validation error — a `required: true` field is missing after transformation |
| Outcome is **NoOp** when you expected writes | Both systems match the ancestor — no drift to reconcile. Modify System A/B data and re-run. |
| Items look duplicated after sync | `set_by_key` identity isn't uniquely naming the rows, or you're using `append` where `set_by_key` was needed. |
| `qty_recv` became a surprising number | `additive` is a CRDT counter: `new = ancestor + (A-ancestor) + (B-ancestor)`. The Winner column shows the derivation. |
| Network error / connection refused | Server isn't running; run `cargo run --manifest-path playground/Cargo.toml` |

---

## Endpoint (for scripting)

```
POST /sync
Content-Type: application/json
{
  "system_a": { ... },
  "system_b": { ... },
  "schema":   { ... },
  "policy":   { "per_field": { ... } },
  "ancestor": { ... } | null,
  "system_a_name": "erp",
  "system_b_name": "inv"
}
```

Response shape:

```
{
  "stages": {
    "transform_a": { "cif": {...} },
    "transform_b": { "cif": {...} },
    "diff":        { "a_vs_ancestor": [...], "b_vs_ancestor": [...], "a_vs_b": [...], "ancestor_used": {...} },
    "policy":      { "would_write": {...} | null, "conflicts": [...] },
    "outcome":     { "kind": "Synced|Escalated|NoOp", "pushed_to": [...], "conflicts": [...] }
  },
  "error": null | "human-readable message"
}
```

When `error` is non-null, stages later than the failure point are `null`.
