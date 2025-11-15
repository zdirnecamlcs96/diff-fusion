# Roadmap: Schema Evolution & Project Scope

## Project Scope & Boundaries

### ✅ IN SCOPE - What diff-fusion DOES

**Core Mission:** Transform, compare, and detect conflicts in JSON data across different formats.

```
System A → CIF ← System B
         ↓
    Compare & Report Conflicts
```

1. **Schema-Driven Transformation**
   - Define CIF format once
   - Transform any format to CIF
   - Linear scaling (n transformers, not n²)

2. **Conflict Detection**
   - Compare CIF objects
   - Report all differences
   - Field-level granularity

3. **Source of Truth Metadata**
   - Document which system owns which fields
   - Provide conflict resolution hints
   - Enable automated decision-making

4. **Type System**
   - CIF type definitions
   - Runtime validation
   - Compile-time safety (traits)

**Role:** Foundation/Building Block for sync systems

### ❌ OUT OF SCOPE - What diff-fusion DOES NOT DO

**NOT a sync engine.** These require different infrastructure:

1. **Bidirectional Write Operations**
   - Writing data back to System A
   - Writing data back to System B
   - Database/API update operations
   - Transaction management

2. **Conflict Resolution Execution**
   - Automatically applying resolution strategies
   - Choosing which value to keep
   - Merging conflicting values
   - Business logic implementation

3. **Sync Orchestration**
   - Scheduling periodic syncs
   - Event-driven triggers
   - Retry mechanisms
   - Dead letter queues

4. **Infrastructure Concerns**
   - Message queues (Kafka, RabbitMQ)
   - Event sourcing
   - Distributed transactions
   - State management
   - Monitoring & alerting

5. **Business Logic**
   - Approval workflows
   - Manual review queues
   - Custom merge strategies
   - Domain-specific rules

**Why out of scope?**

- Different complexity class (stateful vs stateless)
- Requires infrastructure (not a library concern)
- Business-specific (every company has different rules)
- Better served by orchestration tools (Zapier, Fivetran, Airbyte)

**Analogy:**

- `diff-fusion` = `git diff` (shows differences)
- Sync engine = `git merge + push + CI/CD` (does the work)

### 🔗 How Users Build on Top

```rust
// 1. Use diff-fusion to detect conflicts (IN SCOPE)
let diff_fusion = DiffFusion::new(schema);
let report = diff_fusion.compare(&cif_a, &cif_b);

// 2. User implements their own logic (OUT OF SCOPE)
for conflict in report.conflicts {
    match my_business_rules(&conflict) {
        Resolution::UseA => my_db.write_to_system_a(conflict.old_value),
        Resolution::UseB => my_db.write_to_system_b(conflict.new_value),
        Resolution::Manual => my_queue.send_for_review(conflict),
    }
}
```

---

## The Core Challenge

When you introduce an **abstraction layer (CIF)** between systems, you face several challenges:

1. **Schema Evolution**: How do you add/change CIF fields without breaking existing transformers?
2. **Backward Compatibility**: How do old systems continue working when CIF changes?
3. **Source of Truth**: Who decides what's correct when conflicts arise?
4. **Resolution Strategies**: How do you support multiple conflict resolution approaches?

---

## Current State (v0.1.0)

✅ **What we have:**

- Schema-driven transformation to CIF
- Conflict detection between two CIFs
- Manual conflict resolution (user decides)
- Source of truth metadata in schema
- Conflict strategy hints
- Field-level ownership documentation
- Facade API for easy usage

❌ **What we're missing:**

- Schema versioning
- Backward compatibility strategy
- Automated resolution execution (intentionally out of scope)
- Migration paths
- Deprecation warnings

---

## Roadmap

### Phase 0: Scope Clarification (v0.1.1) - Current ✅

**Status:** COMPLETED

**Additions:**

- ✅ Clear scope boundaries documented
- ✅ Source of truth field metadata
- ✅ Conflict strategy enum
- ✅ Context separation patterns
- ✅ Facade API for usability

**Key Files:**

- `src/facade.rs` - High-level user API
- `src/types.rs` - Source of truth & conflict strategies
- `examples/source_of_truth.rs` - Best practices
- `ROADMAP.md` - This document

---

### Phase 1: Schema Versioning (v0.2.0) - Foundation

**Problem:** When you change CIF schema, all existing transformers break.

**Solution:** Add semantic versioning to schemas.

```json
{
  "schema_version": "1.0.0",
  "cif_schema": {
    "product_id": {"type": "string", "required": true},
    "product_price": {"type": "number", "required": true}
  },
  "transformations": {
    "format_a": {
      "compatible_versions": ["1.0.0", "1.1.0"],
      "product_id": {"source_path": "id", "type": "string"}
````

**Implementation:**

```rust
pub struct Schema {
    pub version: Version,  // SemVer
    pub cif_schema: Value,
    pub transformations: HashMap<String, Transformation>,
}

pub fn validate_compatibility(
    schema_version: &Version,
    transformer_versions: &[Version],
) -> Result<(), CompatibilityError>
```

**Benefits:**

- Detect incompatible transformers at runtime
- Fail fast with clear error messages
- Document which versions work together

---

### Phase 2: Field Deprecation (v0.3.0) - Graceful Changes

**Problem:** You want to rename `product_price` → `price` but existing systems still use old field.

**Solution:** Support field aliases and deprecation warnings.

```json
{
  "schema_version": "2.0.0",
  "cif_schema": {
    "product_id": {"type": "string", "required": true},
    "price": {
      "type": "number",
      "required": true,
      "aliases": ["product_price"],  // NEW!
      "deprecated_fields": {
        "product_price": {
          "since": "2.0.0",
          "remove_in": "3.0.0",
          "message": "Use 'price' instead"
        }
      }
    }
  }
}
```

**Implementation:**

```rust
pub struct FieldMetadata {
    pub aliases: Vec<String>,
    pub deprecated_since: Option<Version>,
    pub remove_in: Option<Version>,
    pub migration_guide: Option<String>,
}

pub fn transform_with_warnings(
    source: &Value,
    schema: &Schema,
) -> Result<(Value, Vec<DeprecationWarning>), Error>
```

**Output Example:**

```
⚠️  Warning: Field 'product_price' is deprecated since v2.0.0
   Will be removed in v3.0.0
   Migration: Use 'price' instead

✅ Transformation successful (with 1 deprecation warning)
```

---

### Phase 3: Migration Scripts (v0.4.0) - Safe Transitions

**Problem:** How do you migrate data from schema v1 to v2?

**Solution:** Built-in migration support.

```json
{
  "schema_version": "2.0.0",
  "migrations": [
    {
      "from": "1.0.0",
      "to": "2.0.0",
      "script": "migrations/v1_to_v2.json"
    }
  ]
}
```

**Migration Script (v1_to_v2.json):**

```json
{
  "operations": [
    {
      "type": "rename_field",
      "old": "product_price",
      "new": "price"
    },
    {
      "type": "add_field",
      "name": "currency",
      "default": "USD"
    },
    {
      "type": "transform_field",
      "field": "stock",
      "operation": "ensure_positive"
    }
  ]
}
```

**Implementation:**

```rust
pub fn migrate_cif(
    old_cif: &Value,
    from_version: &Version,
    to_version: &Version,
    schema: &Schema,
) -> Result<Value, MigrationError>

// CLI command
cargo run -- migrate \
  --input old_data.json \
  --from-version 1.0.0 \
  --to-version 2.0.0 \
  --schema schema.json
```

---

### Phase 4: Conflict Resolution Strategies (v0.5.0) - Pluggable Logic

**Problem:** Different scenarios need different resolution strategies.

**Solution:** Support multiple strategies with clear APIs.

```rust
pub trait ConflictResolver {
    fn resolve(&self, conflict: &Conflict) -> ResolvedValue;
}

// Built-in strategies
pub struct LastWriteWins;
pub struct FirstWriteWins;
pub struct ManualResolve;
pub struct HighestValueWins;
pub struct CustomLogic(Box<dyn Fn(&Conflict) -> ResolvedValue>);

// Usage
let resolver = LastWriteWins;
let resolved = resolver.resolve(&conflict);
```

**Schema Configuration:**

```json
{
  "conflict_resolution": {
    "default_strategy": "last-write-wins",
    "field_strategies": {
      "stock": {
        "strategy": "highest-value",
        "reason": "Never decrease stock automatically"
      },
      "price": {
        "strategy": "manual",
        "reason": "Pricing changes need approval"
      },
      "description": {
        "strategy": "last-write-wins",
        "reason": "Latest description is usually correct"
      }
    }
  }
}
```

**CLI Example:**

```bash
# Auto-resolve with strategy
cargo run -- resolve \
  --conflicts conflicts.json \
  --strategy last-write-wins \
  --output resolved.json

# Manual review
cargo run -- resolve \
  --conflicts conflicts.json \
  --strategy manual \
  --interactive
```

---

### Phase 5: Source of Truth Configuration (v0.6.0) - Clear Authority

**Problem:** When System A and System B conflict, who's right?

**Solution:** Declare authority per field type.

```json
{
  "source_of_truth": {
    "inventory": "system_a",
    "pricing": "system_b",
    "product_description": "system_c",
    "customer_data": "latest_timestamp"
  },
  "authority_rules": [
    {
      "field": "stock",
      "authority": "system_a",
      "reason": "ERP is master for inventory"
    },
    {
      "field": "display_price",
      "authority": "system_b",
      "reason": "Shopify controls customer-facing prices"
    },
    {
      "field": "shipping_address",
      "authority": "latest",
      "reason": "Customer can update from any system"
    }
  ]
}
```

**Implementation:**

```rust
pub struct AuthorityRule {
    pub field: String,
    pub authority: Authority,
    pub override_conditions: Vec<Condition>,
}

pub enum Authority {
    System(String),           // Always trust System A
    LatestTimestamp,          // Whoever wrote last
    HighestValue,             // For quantities
    ManualReview,             // Human decides
    Custom(Box<dyn Fn(&Conflict) -> String>),
}

pub fn resolve_with_authority(
    conflict: &Conflict,
    rules: &[AuthorityRule],
) -> ResolvedValue
```

---

### Phase 6: Backward Compatibility Layer (v0.7.0) - Bridge Old & New

**Problem:** Old transformers can't handle new CIF schema.

**Solution:** Automatic downgrade transformations.

```rust
pub struct CompatibilityLayer {
    pub target_version: Version,
    pub current_version: Version,
}

impl CompatibilityLayer {
    // Automatically strip new fields for old clients
    pub fn downgrade(&self, cif: &Value) -> Value {
        match (self.current_version, self.target_version) {
            (v2, v1) => {
                // Remove v2-only fields
                // Rename v2 fields to v1 names
                // Apply backwards transformations
            }
        }
    }
}
```

**Schema Configuration:**

```json
{
  "schema_version": "2.0.0",
  "backward_compatibility": {
    "support_versions": ["1.0.0", "1.1.0", "1.2.0"],
    "field_mappings": {
      "1.x": {
        "price": "product_price",  // Map v2 back to v1
        "currency": null            // Remove in v1
      }
    }
  }
}
```

**Automatic Bridge:**

```rust
// Transformer written for v1.0.0
let transformer_v1 = load_transformer("system_a", "1.0.0");

// Current schema is v2.0.0
let schema_v2 = load_schema("2.0.0");

// Automatically bridge
let cif_v2 = transform_to_cif(&data, &schema_v2, "system_a")?;
let cif_v1 = schema_v2.downgrade_to(&transformer_v1.version)?;

// v1 transformer can now work with v2 data! ✅
```

---

### Phase 7: Deprecation Timeline (v1.0.0) - Clear Sunset Path

**Problem:** When can you safely remove deprecated fields?

**Solution:** Enforce deprecation timelines with warnings.

```json
{
  "deprecation_policy": {
    "warning_period_months": 6,
    "grace_period_months": 12,
    "notification_channels": ["changelog", "api_warnings", "email"]
  },
  "deprecated_fields": {
    "product_price": {
      "deprecated_date": "2025-06-01",
      "removal_date": "2026-06-01",
      "migration_path": "Use 'price' field instead",
      "affected_transformers": ["format_a", "format_b"]
    }
  }
}
```

**Runtime Behavior:**

```rust
pub enum DeprecationStatus {
    Active,
    WarningPeriod { months_remaining: u32 },
    GracePeriod { months_remaining: u32 },
    Removed,
}

pub fn check_deprecation_status(
    field: &str,
    schema: &Schema,
) -> DeprecationStatus {
    // Calculate based on current date vs deprecation timeline
}

// Example output
⚠️  DEPRECATION WARNING:
    Field 'product_price' will be removed in 3 months (2026-03-01)
    Affected transformers: format_a, format_b
    Migration guide: https://docs.../migration-v1-to-v2

    Update your transformer before removal date!
```

---

## Implementation Priority

### Critical Path (Must Have)

1. **Schema Versioning** (Phase 1) - Foundation for everything
2. **Conflict Resolution Strategies** (Phase 4) - Core business value
3. **Source of Truth** (Phase 5) - Solves authority problem

### Important (Should Have)

4. **Field Deprecation** (Phase 2) - Enables safe evolution
5. **Backward Compatibility** (Phase 6) - Eases adoption

### Nice to Have

6. **Migration Scripts** (Phase 3) - Advanced use cases
7. **Deprecation Timeline** (Phase 7) - Large-scale deployments

---

## Example: Schema Evolution Journey

### v1.0.0 - Initial Release

```json
{
  "schema_version": "1.0.0",
  "cif_schema": {
    "product_id": {"type": "string", "required": true},
    "product_price": {"type": "number", "required": true}
  }
}
```

### v1.1.0 - Add Optional Field (Non-breaking)

```json
{
  "schema_version": "1.1.0",
  "cif_schema": {
    "product_id": {"type": "string", "required": true},
    "product_price": {"type": "number", "required": true},
    "currency": {"type": "string", "required": false, "default": "USD"}
  }
}
```

✅ Old transformers still work (currency is optional)

### v2.0.0 - Rename Field (Breaking)

```json
{
  "schema_version": "2.0.0",
  "cif_schema": {
    "product_id": {"type": "string", "required": true},
    "price": {
      "type": "number",
      "required": true,
      "aliases": ["product_price"],
      "deprecated_fields": {
        "product_price": {
          "since": "2.0.0",
          "remove_in": "3.0.0"
        }
      }
    },
    "currency": {"type": "string", "required": false, "default": "USD"}
  },
  "backward_compatibility": {
    "support_versions": ["1.0.0", "1.1.0"]
  }
}
```

⚠️  Old transformers get deprecation warnings but still work

### v3.0.0 - Remove Deprecated (Breaking)

```json
{
  "schema_version": "3.0.0",
  "cif_schema": {
    "product_id": {"type": "string", "required": true},
    "price": {"type": "number", "required": true},
    "currency": {"type": "string", "required": true}  // Now required!
  },
  "migrations": [{
    "from": "2.0.0",
    "to": "3.0.0",
    "operations": [
      {"type": "remove_field", "name": "product_price"},
      {"type": "require_field", "name": "currency"}
    ]
  }]
}
```

❌ Old transformers MUST upgrade or use compatibility layer

---

## Design Principles

1. **Fail Fast**: Detect incompatibilities at startup, not runtime
2. **Warn Early**: Deprecation warnings 6-12 months before removal
3. **Provide Path**: Every breaking change includes migration guide
4. **Support Old**: Maintain backward compatibility for N-2 versions
5. **Document Everything**: Changelog with version compatibility matrix

---

## Next Steps

**For v0.2.0 (Next Release):**

1. Add `schema_version` field to schema.json
2. Implement version validation in `transform.rs`
3. Add unit tests for version compatibility
4. Update documentation with versioning examples
5. Add CLI flag: `--strict-version` to enforce exact matches

**Quick Win:** Start with semantic versioning validation. This gives you:

- Foundation for all future work
- Immediate value (catch compatibility issues)
- Low implementation cost (~100 lines of code)

Would you like me to implement Phase 1 (Schema Versioning) as a starting point? 🚀

---

## For Users Building Sync Engines

**If you need bidirectional sync, here's how to build on top of diff-fusion:**

### Architecture Pattern

```
┌──────────────────────────────────────────────────┐
│  Your Sync Orchestrator Application              │
│                                                  │
│  ┌────────────────────────────────────────┐     │
│  │  diff-fusion (This Library)            │     │
│  │  • Transform to CIF                    │     │
│  │  • Detect conflicts                    │     │
│  │  • Report differences                  │     │
│  └────────────────────────────────────────┘     │
│                    ↓                             │
│  ┌────────────────────────────────────────┐     │
│  │  Your Business Logic Layer             │     │
│  │  • Apply resolution strategies         │     │
│  │  • Implement retry logic               │     │
│  │  • Handle transactions                 │     │
│  └────────────────────────────────────────┘     │
│                    ↓                             │
│  ┌────────────────────────────────────────┐     │
│  │  Your Data Access Layer                │     │
│  │  • Write to System A API               │     │
│  │  • Write to System B database          │     │
│  │  • Manage connections                  │     │
│  └────────────────────────────────────────┘     │
└──────────────────────────────────────────────────┘
```

### Recommended Tools & Patterns

**For Sync Orchestration:**

- **Temporal** - Workflow orchestration with retries
- **Apache Airflow** - Scheduled data pipelines
- **AWS Step Functions** - Serverless orchestration
- **Celery** - Distributed task queue

**For Conflict Resolution:**

- **Redis** - Locking mechanism
- **PostgreSQL** - Transaction management
- **Event Sourcing** - Track all changes
- **CQRS Pattern** - Separate reads/writes

**For Monitoring:**

- **Prometheus** - Metrics
- **Grafana** - Dashboards
- **Sentry** - Error tracking
- **DataDog** - Full observability

### Example Integration

```rust
use diff_fusion::DiffFusion;
use tokio::time::{sleep, Duration};

// Your sync engine that uses diff-fusion
async fn sync_engine(
    system_a_client: &SystemAClient,
    system_b_client: &SystemBClient,
    diff_fusion: &DiffFusion,
) -> Result<(), SyncError> {
    loop {
        // 1. Fetch data from both systems
        let data_a = system_a_client.fetch().await?;
        let data_b = system_b_client.fetch().await?;

        // 2. Use diff-fusion to detect conflicts
        let report = diff_fusion.transform_and_compare(
            &data_a, "system_a",
            &data_b, "system_b",
        )?;

        // 3. Your business logic resolves conflicts
        for conflict in report.conflicts {
            let resolution = resolve_conflict(&conflict)?;

            // 4. Your code writes back to systems
            match resolution {
                Resolution::UseA(value) => {
                    system_b_client.update(&conflict.path, value).await?;
                }
                Resolution::UseB(value) => {
                    system_a_client.update(&conflict.path, value).await?;
                }
                Resolution::Manual => {
                    send_to_review_queue(conflict).await?;
                }
            }
        }

        // 5. Your orchestration decides when to sync again
        sleep(Duration::from_secs(60)).await;
    }
}

// Your business logic
fn resolve_conflict(conflict: &Conflict) -> Result<Resolution, Error> {
    // Check source of truth metadata from schema
    match conflict.path.as_str() {
        "price" => Ok(Resolution::UseB(conflict.new_value)), // Pricing system wins
        "stock" => Ok(Resolution::UseA(conflict.old_value)), // Inventory wins
        _ => Ok(Resolution::Manual), // Human review
    }
}
```

### Commercial Alternatives

If building your own is too complex, consider:

1. **Fivetran** ($100-$5000+/month) - Automated data connectors
2. **Airbyte** (Open Source + Cloud) - 300+ pre-built connectors
3. **Zapier** ($20-$600/month) - No-code automation
4. **Mulesoft** (Enterprise) - Full integration platform
5. **Segment** (Customer data) - Specialized for user tracking

**When to build vs buy:**

- Build: Unique business logic, cost-sensitive, need full control
- Buy: Standard integrations, time-sensitive, need support

### Open Source Reference Implementations

Example projects that could use diff-fusion as a foundation:

```rust
// 1. Simple periodic sync
// https://github.com/your-org/simple-sync
// Uses: diff-fusion + cron + PostgreSQL

// 2. Event-driven sync
// https://github.com/your-org/event-sync
// Uses: diff-fusion + Kafka + Redis

// 3. API gateway sync
// https://github.com/your-org/gateway-sync
// Uses: diff-fusion + Axum + webhook handlers
```

**We may provide reference implementations in the future, but they will be separate projects.**

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for:

- Coding principles (Functional > SOLID for Rust)
- Clean architecture guidelines
- How to submit issues and PRs

---

## Questions?

**Scope Questions:**

- "Can diff-fusion sync my databases?" → No, but you can build that with it
- "Does it handle retries?" → No, that's orchestration layer
- "Can it merge conflicts?" → It detects conflicts; you decide how to merge

**Implementation Questions:**

- "Which phase should I start with?" → Phase 1 (Schema Versioning)
- "Can I contribute?" → Yes! See CONTRIBUTING.md
- "Is this production-ready?" → Core features yes, advanced features coming

Open an issue on GitHub for any other questions!
