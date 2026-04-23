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
| `append` | Arrays — concatenate both sides' additions | — |
| `state_machine` | Enum fields — only listed `(from, to)` transitions are accepted; anything else escalates | `transitions` |

Fields not listed in `per_field` that changed on both sides will produce a
`NoPolicy` conflict.

---

## The five pipeline stages

1. **Transform A → CIF** — applies `schema.transformations.<system_a_name>` to your System A JSON. Output: canonical CIF for A.
2. **Transform B → CIF** — same for System B.
3. **3-way Diff** — computes three diffs: `A vs Ancestor`, `B vs Ancestor`, `A vs B`. Missing ancestor is treated as `{}`.
4. **Policy Resolution** — dry-run through every field that changed, applying your per-field policies. Shows the `would_write` value and any conflicts it would have escalated.
5. **Outcome** — the real sync. Result is one of:
   - **Synced** — resolution was clean, both sides got written (`pushed_to`).
   - **Escalated** — at least one conflict survived; nothing was written.
   - **NoOp** — neither side changed since the ancestor.

Stages animate in order on each run; each card expands to show its intermediate data.

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

---

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| `Transform System A failed: Format 'X' not found in schema` | **System A name** doesn't match any key in `schema.transformations` |
| `Invalid policy for 'X': unknown variant ...` | Typo in `kind` (must be `owned_by`, `additive`, `append`, `state_machine`) |
| `Sync failed: ...` | Usually a schema validation error — a `required: true` field is missing after transformation |
| Outcome is **NoOp** when you expected writes | Both systems match the ancestor — no drift to reconcile. Modify System A/B data and re-run. |
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
