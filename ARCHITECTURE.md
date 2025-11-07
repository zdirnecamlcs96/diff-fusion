# Architecture: CIF for Bidirectional Sync with Conflict Resolution

## Overview

This document explains how to integrate CIF into a bidirectional synchronization system with conflict detection and resolution.

---

## Current System (Without CIF)

```
┌─────────┐           ┌─────────┐
│System A │◄─REST API─►│System B │
└────┬────┘           └────┬────┘
     │                     │
     ▼                     ▼
┌─────────┐           ┌─────────┐
│  DB A   │           │  DB B   │
└─────────┘           └─────────┘

Flow:
1. Fetch from System B → Transform to A format → Save to DB A
2. Transform A to B payload → Send to System B → Save to DB B
3. Detect conflicts (compare A format vs B format - COMPLEX!)
4. Resolve conflicts → Save back to A
```

**Problem:** Comparing different formats is complex. Adding System C requires new transformers and conflict detection logic.

---

## Proposed System (With CIF)

```
┌─────────┐           ┌─────────┐           ┌─────────┐
│System A │           │   CIF   │           │System B │
└────┬────┘           └────┬────┘           └────┬────┘
     │                     │                     │
     │    A→CIF Transform  │  CIF→B Transform    │
     └─────────►┌──────────┴──────────┐◄─────────┘
                │  CIF Layer (DB)     │
                │  - Stores canonical │
                │  - Conflict detect  │
                │  - Version history  │
                └──────────┬──────────┘
                           │
                    diff-fusion tool
                    (detect conflicts)
                           │
                           ▼
                ┌─────────────────┐
                │ Resolve Conflict│
                │ (business rules)│
                └─────────────────┘
```

### Flow with CIF

1. **Inbound Sync (B → A)**

   ```
   System B → CIF (using schema) → Store in CIF DB
   Compare: CIF (latest) vs CIF (previous) using diff-fusion
   If conflict → Resolve → CIF → System A
   ```

2. **Outbound Sync (A → B)**

   ```
   System A → CIF (using schema) → Store in CIF DB
   Compare: CIF (A's view) vs CIF (B's view) using diff-fusion
   If conflict → Resolve → CIF → System B
   ```

3. **Conflict Detection**

   ```
   diff-fusion compare cif_a.json cif_b.json --schema schema.json

   Output:
   - No conflicts: Proceed with sync
   - Conflicts found: Trigger resolution strategy
   ```

---

## Key Design Principles (Isomorphic & Simple)

### 1. **CIF as Single Source of Truth**

- Store only CIF in database (not original formats)
- Transform on-the-fly when syncing to systems

### 2. **Reversible Transformations**

```rust
// schema.json includes BOTH directions
{
  "cif_schema": { ... },
  "transformations": {
    "system_a_to_cif": { ... },
    "cif_to_system_a": { ... },  // Reverse mapping
    "system_b_to_cif": { ... },
    "cif_to_system_b": { ... }   // Reverse mapping
  }
}
```

### 3. **Git-Like Conflict Detection**

```rust
// Pseudocode for conflict detection
fn detect_conflicts(local_cif: Value, remote_cif: Value) -> Vec<Conflict> {
    let diffs = diff_fusion::compare_json(&local_cif, &remote_cif);

    diffs.into_iter()
        .filter(|diff| !is_auto_mergeable(diff))
        .collect()
}

fn is_auto_mergeable(diff: &Diff) -> bool {
    match diff {
        Diff::Added(_) => true,        // Remote added field → accept
        Diff::Removed(_) => true,      // Remote removed field → accept
        Diff::Modified(field, old, new) => {
            // Auto-merge if timestamp shows remote is newer
            remote_timestamp > local_timestamp
        }
    }
}
```

### 4. **Conflict Resolution Strategies**

```rust
enum ResolutionStrategy {
    LastWriteWins,           // Use timestamp
    RemoteWins,              // Always take remote
    LocalWins,               // Always take local
    Manual,                  // Require user intervention
    Custom(Box<dyn Fn(&Conflict) -> Value>),  // Business logic
}
```

---

## Implementation Roadmap

### Phase 1: Add Reverse Transformations (Current Tool)

```bash
# Extend schema.json to support bidirectional transforms
{
  "transformations": {
    "format_a": {
      "to_cif": { "product_id": "id", ... },
      "from_cif": { "id": "product_id", ... }  // NEW!
    }
  }
}
```

### Phase 2: Add Conflict Detection

```bash
# New command
cargo run -- detect-conflicts \
  --local local_cif.json \
  --remote remote_cif.json \
  --schema schema.json \
  --strategy last-write-wins
```

### Phase 3: Add Conflict Resolution

```bash
# New command
cargo run -- resolve \
  --conflicts conflicts.json \
  --strategy manual \
  --output resolved_cif.json
```

---

## Example: E-Commerce Product Sync

### System A (Internal ERP)

```json
{
  "product_id": "P123",
  "stock": 50,
  "last_updated": "2025-11-04T10:00:00Z"
}
```

### System B (Shopify)

```json
{
  "id": "P123",
  "inventory_quantity": 45,
  "updated_at": "2025-11-04T10:30:00Z"
}
```

### CIF (After Transformation)

```json
// From System A
{
  "id": "P123",
  "quantity": 50,
  "timestamp": "2025-11-04T10:00:00Z"
}

// From System B
{
  "id": "P123",
  "quantity": 45,
  "timestamp": "2025-11-04T10:30:00Z"
}
```

### Conflict Detection (using diff-fusion)

```bash
cargo run -- diff cif_a.json cif_b.json --schema schema.json

Output:
quantity: Number(50) → Number(45)  # CONFLICT!
timestamp: "2025-11-04T10:00:00Z" → "2025-11-04T10:30:00Z"
```

### Resolution (Last-Write-Wins)

```rust
// System B is newer (10:30 > 10:00)
// Take System B's value: quantity = 45
// Sync back to System A
```

---

## Advantages of This Approach

1. **Isomorphic**: Same transformation logic for A→CIF and CIF→A (just reverse schema)
2. **Simple Conflict Detection**: Always compare CIF vs CIF (your tool already does this!)
3. **Scalable**: Adding System C requires only 2 transformers (C→CIF, CIF→C)
4. **Git-Like**: Familiar mental model (merge, conflict, resolve)
5. **Database Simplification**: Store only CIF, not multiple formats

---

## When NOT to Use CIF for Your System

❌ **Don't use CIF if:**

- You only have 2 systems (A and B) with no plans to add more
- Transformations are trivial (1:1 field mapping)
- Real-time sync required (CIF adds transformation overhead)
- Systems are strongly coupled by design

✅ **DO use CIF if:**

- Planning to add System C, D, E later
- Complex field mappings (nested objects, type conversions)
- Need centralized conflict detection/resolution
- Want to decouple system dependencies

---

## Next Steps

To make your `diff-fusion` tool support this workflow, consider adding:

1. **Bidirectional schema support** (to_cif + from_cif)
2. **Conflict detection command** (not just diff)
3. **Resolution strategies** (last-write-wins, manual, etc.)
4. **Timestamp handling** (for auto-merge decisions)
5. **Output conflicts in structured format** (JSON/YAML for automation)

Let me know if you want to implement any of these features!
