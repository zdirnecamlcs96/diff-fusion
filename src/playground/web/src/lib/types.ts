/// Wire types — match the Rust `playground/src/dto.rs` and
/// `src/ports/observer.rs` exactly. Keep field names byte-identical.

export type Json = unknown;

export interface FieldDiff {
  path: string;
  left: Json;
  right: Json;
}

export interface DiffStage {
  a_vs_ancestor: FieldDiff[];
  b_vs_ancestor: FieldDiff[];
  a_vs_b: FieldDiff[];
  ancestor_used: Json;
}

export interface ConflictDto {
  path: string;
  reason: string;
  class: string;
}

export interface PolicyStage {
  would_write: Json | null;
  conflicts: ConflictDto[];
}

export type OutcomeKind = "NoOp" | "Synced" | "Escalated";

export interface OutcomeDto {
  kind: OutcomeKind;
  pushed_to: string[];
  conflicts: ConflictDto[];
}

export interface Timings {
  transform_a_ms?: number | null;
  transform_b_ms?: number | null;
  diff_ms?: number | null;
  policy_ms?: number | null;
  outcome_ms?: number | null;
  total_ms: number;
}

export interface StagesDto {
  transform_a?: { cif: Json } & Record<string, unknown>;
  transform_b?: { cif: Json } & Record<string, unknown>;
  diff?: DiffStage;
  policy?: PolicyStage;
  outcome?: OutcomeDto;
  timings?: Timings;
}

export interface SyncResponse {
  stages: StagesDto;
  error: string | null;
}

/* ----- Capture wire format (Rust src/ports/observer.rs) ----------------- */

export interface SideCapture {
  system: string;
  canonical_view: Json;
  version: string | null;
}

export interface Capture {
  entity_type: string;
  canonical_id: string;
  side_a: SideCapture;
  side_b: SideCapture;
}

export interface CaptureSummary {
  capture_id: string;
  entity_type: string;
  canonical_id: string;
  saved_at_ms: number;
}

/* ----- Demo /sync progress events --------------------------------------- */
/* Wire format from playground/src/dto.rs::ProgressEvent — one event per
   pipeline stage, streamed over /api/sync/:sync_id/stream while the
   POST /sync request is in flight. */

export type ProgressEvent =
  | { stage: "transform_a"; data: { cif: Json }; duration_ms: number }
  | { stage: "transform_b"; data: { cif: Json }; duration_ms: number }
  | { stage: "diff"; data: DiffStage; duration_ms: number }
  | { stage: "policy"; data: PolicyStage; duration_ms: number }
  | { stage: "outcome"; data: OutcomeDto; duration_ms: number }
  | { stage: "error"; message: string; partial: StagesDto };

/* ----- Demo form context — assembled by DemoForm, consumed by ----------- */
/* ----- FieldChangelog so it can show "from System A → to written".       */

export interface RunContext {
  ancestor: Record<string, Json>;
  cif_a: Record<string, Json>;
  cif_b: Record<string, Json>;
  would_write: Record<string, Json> | null;
  system_a_name: string;
  system_b_name: string;
  policy_per_field: Record<string, Json>;
}

export interface ResolvedRun {
  stages: StagesDto;
  context: RunContext;
}

/* ----- Saved tests (Rust playground/src/runs.rs::TestRecord) ------------ */
/* Wizard-authored tests, stored verbatim as raw textarea strings so that */
/* reload round-trips exactly (no JSON re-formatting drift).               */

export interface TestRecord {
  name: string;
  cif_schema: string;
  policy: string;
  transformer_a: string;
  transformer_b: string;
  system_a: string;
  system_b: string;
  ancestor: string;
  system_a_name: string;
  system_b_name: string;
  /** Outcome kind from the most recent run, if any. */
  last_outcome?: string | null;
}

export interface TestSummary {
  test_id: string;
  name: string;
  system_a_name: string;
  system_b_name: string;
  last_outcome: string | null;
  saved_at_ms: number;
}

/* ----- Observer configs (Rust playground/src/runs.rs::ObserverConfig) -- */
/* Inbound producer label: each entry tells the playground "expect       */
/* captures with this capture_id to come from a producer named name".    */

export interface ObserverConfig {
  name: string;
  capture_id: string;
}

export interface ObserverSummary {
  observer_id: string;
  name: string;
  capture_id: string;
  saved_at_ms: number;
  /** Wall-clock of the last matching capture POST, if any. */
  last_seen_ms: number | null;
}
