/// Thin fetch wrappers for the Rust playground's API surface.

import type {
  Capture,
  CaptureSummary,
  ObserverConfig,
  ObserverSummary,
  SyncResponse,
  TestRecord,
  TestSummary,
} from "./types";

export async function postSync(payload: unknown): Promise<SyncResponse> {
  const res = await fetch("/sync", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(payload),
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return (await res.json()) as SyncResponse;
}

export async function postSuggest(schema: unknown): Promise<{ per_field: Record<string, unknown> }> {
  const res = await fetch("/api/suggest", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ schema }),
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return (await res.json()) as { per_field: Record<string, unknown> };
}

export async function getCaptures(): Promise<CaptureSummary[]> {
  const res = await fetch("/api/captures");
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return (await res.json()) as CaptureSummary[];
}

export async function getCapture(captureId: string): Promise<Capture> {
  const res = await fetch(`/api/captures/${encodeURIComponent(captureId)}`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return (await res.json()) as Capture;
}

export async function putTest(testId: string, record: TestRecord): Promise<void> {
  const res = await fetch(`/api/tests/${encodeURIComponent(testId)}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(record),
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
}

export async function getTests(): Promise<TestSummary[]> {
  const res = await fetch("/api/tests");
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return (await res.json()) as TestSummary[];
}

export async function getTest(testId: string): Promise<TestRecord> {
  const res = await fetch(`/api/tests/${encodeURIComponent(testId)}`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return (await res.json()) as TestRecord;
}

export async function getObservers(): Promise<ObserverSummary[]> {
  const res = await fetch("/api/observers");
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return (await res.json()) as ObserverSummary[];
}

export async function putObserver(
  observerId: string,
  config: ObserverConfig,
): Promise<void> {
  const res = await fetch(`/api/observers/${encodeURIComponent(observerId)}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(config),
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
}

export async function deleteObserver(observerId: string): Promise<void> {
  const res = await fetch(`/api/observers/${encodeURIComponent(observerId)}`, {
    method: "DELETE",
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
}
