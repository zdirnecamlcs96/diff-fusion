/// <reference types="node" />
/**
 * Demo: ship a single `Capture` from a TypeScript program to the
 * diff-fusion playground.
 *
 * The playground accepts captures from any HTTP client — Rust, TS, Python,
 * curl. This file shows the contract:
 *
 *   POST {endpoint}/api/captures/{captureId}
 *   body: { entity_type, canonical_id, side_a: {...}, side_b: {...} }
 *
 * Run it:
 *
 *   1. terminal A:  cargo run -p playground   (from the repo root)
 *   2. terminal B:  npx tsx ts/examples/observerHttp.ts
 *   3. browser:     http://localhost:3000  → click the capture id
 *
 * Override the endpoint with $PLAYGROUND_URL and the capture id with
 * $OBSERVE_CAPTURE_ID.
 */

type Json = unknown;

interface SideCapture {
  system: string;
  canonical_view: Json;
  version: string | null;
}

interface Capture {
  entity_type: string;
  canonical_id: string;
  side_a: SideCapture;
  side_b: SideCapture;
}

async function postCapture(endpoint: string, captureId: string, capture: Capture): Promise<void> {
  const url = `${endpoint.replace(/\/+$/, "")}/api/captures/${encodeURIComponent(captureId)}`;
  const res = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(capture),
  });
  if (!res.ok) {
    throw new Error(`POST ${url} → ${res.status}`);
  }
}

async function main(): Promise<void> {
  const endpoint = process.env.PLAYGROUND_URL ?? "http://localhost:3000";
  const captureId = process.env.OBSERVE_CAPTURE_ID ?? `ts-demo-${Date.now()}`;

  console.log(`posting capture to ${endpoint} as id ${captureId}`);

  const capture: Capture = {
    entity_type: "purchase_order",
    canonical_id: "PO-42",
    side_a: {
      system: "erp",
      canonical_view: { price: 18, qty_received: 0 },
      version: "1",
    },
    side_b: {
      system: "warehouse",
      canonical_view: { price: 12, qty_received: 3 },
      version: "1",
    },
  };

  await postCapture(endpoint, captureId, capture);
  console.log("done — open the playground and click the capture id");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
