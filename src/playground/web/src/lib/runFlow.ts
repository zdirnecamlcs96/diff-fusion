/// Pure helpers + the runSync orchestrator extracted from the legacy
/// DemoForm so the stepper-driven NewTest view (and any future flow)
/// can reuse them without dragging the old single-page form along.
///
/// Side effects are confined to `runSync`: it opens an EventSource,
/// drives `runState` (the dialog), and POSTs /sync. Everything else
/// (parsing, schema assembly, suggest) is pure.

import { postSync, postSuggest } from "./api";
import { runState, showRun } from "./runState.svelte";
import type { Json, ProgressEvent, SyncResponse } from "./types";

export interface FormFields {
  systemA: string;
  systemB: string;
  cifSchema: string;
  policy: string;
  ancestor: string;
  transformerA: string;
  transformerB: string;
  systemAName: string;
  systemBName: string;
}

export type StatusTone = "" | "ok" | "err";
export type StatusFn = (msg: string, tone?: StatusTone) => void;

export function parseJsonOrNull(raw: string, label: string, requireNonEmpty = true): Json {
  const s = raw.trim();
  if (!s) {
    if (requireNonEmpty) throw new Error(`${label} is empty`);
    return null;
  }
  try {
    return JSON.parse(s);
  } catch (e: any) {
    throw new Error(`${label}: ${e.message}`);
  }
}

export function buildSchema(f: FormFields): Json {
  const cifRaw = parseJsonOrNull(f.cifSchema, "CIF schema") as any;
  const cif = cifRaw && cifRaw.cif_schema ? cifRaw.cif_schema : cifRaw;
  const tA = parseJsonOrNull(f.transformerA, "Transformer A");
  const tB = parseJsonOrNull(f.transformerB, "Transformer B");
  const aName = f.systemAName.trim() || "system_a";
  const bName = f.systemBName.trim() || "system_b";
  return {
    cif_schema: cif,
    transformations: { [aName]: tA, [bName]: tB },
  };
}

export interface RunSyncOptions {
  fields: FormFields;
  setStatus: StatusFn;
  title?: string;
}

/** Run one sync pipeline: open the dialog, subscribe to SSE, POST /sync,
 *  fold each progress event into `runState` so the dialog animates, and
 *  finally surface the synchronous response's outcome (or error). */
export async function runSync(opts: RunSyncOptions): Promise<SyncResponse | null> {
  const { fields: f, setStatus, title = "New Test · Run Sync" } = opts;
  setStatus("Running…");

  const syncId = `demo-${Date.now()}`;
  let policyJson: any;
  let payload: any;
  try {
    policyJson = parseJsonOrNull(f.policy, "Policy") as any;
    payload = {
      system_a: parseJsonOrNull(f.systemA, "System A"),
      system_b: parseJsonOrNull(f.systemB, "System B"),
      schema: buildSchema(f),
      policy: policyJson,
      ancestor: parseJsonOrNull(f.ancestor, "Ancestor", false),
      system_a_name: f.systemAName.trim() || "system_a",
      system_b_name: f.systemBName.trim() || "system_b",
      run_id: syncId,
    };
  } catch (e: any) {
    setStatus(e.message, "err");
    return null;
  }

  showRun({
    stages: {},
    context: {
      ancestor: {},
      cif_a: {},
      cif_b: {},
      would_write: null,
      system_a_name: payload.system_a_name,
      system_b_name: payload.system_b_name,
      policy_per_field: (policyJson?.per_field ?? {}) as Record<string, unknown>,
    },
    title,
    subtitle: `${payload.system_a_name} ↔ ${payload.system_b_name}`,
  });

  // Subscribe BEFORE posting so no events are missed (the registry also
  // keeps a replay buffer, but ordering on connect is cleaner this way).
  const src = new EventSource(`/api/sync/${encodeURIComponent(syncId)}/stream`);
  src.addEventListener("progress", (e) => {
    let ev: ProgressEvent;
    try {
      ev = JSON.parse((e as MessageEvent).data);
    } catch {
      return;
    }
    applyProgress(ev, setStatus);
  });

  try {
    const res = await postSync(payload);
    if (res.error) {
      setStatus(res.error, "err");
    } else if (res.stages.outcome) {
      setStatus(`Done · ${res.stages.outcome.kind}`, "ok");
    }
    return res;
  } catch (e: any) {
    setStatus(`Network error: ${e.message}`, "err");
    return null;
  } finally {
    setTimeout(() => src.close(), 250);
  }
}

/** Fold one progress event into the live runState the dialog reads. */
function applyProgress(ev: ProgressEvent, setStatus: StatusFn): void {
  if (!runState.stages.timings) {
    runState.stages.timings = { total_ms: 0 };
  }
  const t = runState.stages.timings;
  switch (ev.stage) {
    case "transform_a":
      runState.stages.transform_a = ev.data;
      t.transform_a_ms = ev.duration_ms;
      if (runState.context) runState.context.cif_a = ev.data.cif as Record<string, unknown>;
      return;
    case "transform_b":
      runState.stages.transform_b = ev.data;
      t.transform_b_ms = ev.duration_ms;
      if (runState.context) runState.context.cif_b = ev.data.cif as Record<string, unknown>;
      return;
    case "diff":
      runState.stages.diff = ev.data;
      t.diff_ms = ev.duration_ms;
      if (runState.context) runState.context.ancestor = ev.data.ancestor_used as Record<string, unknown>;
      return;
    case "policy":
      runState.stages.policy = ev.data;
      t.policy_ms = ev.duration_ms;
      if (runState.context) runState.context.would_write = ev.data.would_write as Record<string, unknown> | null;
      return;
    case "outcome":
      runState.stages.outcome = ev.data;
      t.outcome_ms = ev.duration_ms;
      t.total_ms =
        Math.max(t.transform_a_ms ?? 0, t.transform_b_ms ?? 0) +
        (t.diff_ms ?? 0) +
        (t.policy_ms ?? 0) +
        (t.outcome_ms ?? 0);
      return;
    case "error":
      setStatus(ev.message, "err");
      return;
    default: {
      const _exhaustive: never = ev;
      return _exhaustive;
    }
  }
}

/** Call /api/suggest with whatever's in the schema textarea and return
 *  a stringified `{ per_field: ... }` JSON ready to drop into the policy
 *  field. Throws on parse / network error so the caller sets status. */
export async function suggestPolicyFromSchema(cifSchemaRaw: string): Promise<string> {
  const cifRaw = parseJsonOrNull(cifSchemaRaw, "CIF schema") as any;
  const schemaForSuggest = cifRaw && cifRaw.cif_schema ? cifRaw : { cif_schema: cifRaw };
  const res = await postSuggest(schemaForSuggest);
  return JSON.stringify({ per_field: res.per_field }, null, 2);
}
