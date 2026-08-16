/// Pure helpers for rendering pipeline stages, diff cells, and policy
/// summaries. Translated from the legacy app.js so the visual output is
/// equivalent. Functions return data, not DOM — Svelte components do
/// the rendering.

import type { DiffStage, OutcomeDto, PolicyStage, StagesDto } from "./types";

export type Json = unknown;

export function fmtCell(v: Json): string {
  if (v === undefined) return "—";
  if (v === null) return "null";
  if (typeof v === "string") return JSON.stringify(v);
  if (typeof v === "number" || typeof v === "boolean") return String(v);
  return JSON.stringify(v, null, 2);
}

export function stableEq(a: Json, b: Json): boolean {
  if (a === b) return true;
  if (a === undefined || b === undefined) return false;
  return JSON.stringify(a) === JSON.stringify(b);
}

/* --- diff (LCS line-level) --------------------------------------------- */

export type DiffLine = { type: "ctx" | "add" | "rm"; text: string };

export function lineDiff(fromStr: string, toStr: string): DiffLine[] {
  const from = (fromStr === "—" || fromStr == null ? "" : String(fromStr)).split("\n");
  const to = (toStr === "—" || toStr == null ? "" : String(toStr)).split("\n");
  const m = from.length;
  const n = to.length;
  const dp: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
  for (let i = m - 1; i >= 0; i--) {
    for (let j = n - 1; j >= 0; j--) {
      const row = dp[i]!;
      const next = dp[i + 1]!;
      row[j] = from[i] === to[j] ? next[j + 1]! + 1 : Math.max(next[j]!, row[j + 1]!);
    }
  }
  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < m && j < n) {
    if (from[i] === to[j]) {
      out.push({ type: "ctx", text: to[j]! });
      i++;
      j++;
    } else if (dp[i + 1]![j]! >= dp[i]![j + 1]!) {
      out.push({ type: "rm", text: from[i]! });
      i++;
    } else {
      out.push({ type: "add", text: to[j]! });
      j++;
    }
  }
  while (i < m) out.push({ type: "rm", text: from[i++]! });
  while (j < n) out.push({ type: "add", text: to[j++]! });
  return out;
}

export interface RenderedCell {
  asPlain: string | null;
  lines: DiffLine[];
}

/** Diff cell for A / B / Written: skips pure removals (those belong on
 *  the Ancestor column) and emits only context + additions. */
export function diffCellLines(fromText: string, toText: string): RenderedCell {
  const diff = lineDiff(fromText, toText);
  const anyDelta = diff.some((d) => d.type !== "ctx");
  if (!anyDelta) return { asPlain: toText, lines: [] };
  return { asPlain: null, lines: diff.filter((d) => d.type !== "rm") };
}

/** Diff cell for the Ancestor column: skips additions, marks removals. */
export function ancestorDiffCellLines(ancestorText: string, writtenText: string): RenderedCell {
  const diff = lineDiff(ancestorText, writtenText);
  const anyDelta = diff.some((d) => d.type !== "ctx");
  if (!anyDelta) return { asPlain: ancestorText, lines: [] };
  const lines: DiffLine[] = [];
  for (const d of diff) {
    if (d.type === "add") continue;
    lines.push({ type: d.type === "rm" ? "rm" : "ctx", text: d.text });
  }
  return { asPlain: null, lines };
}

/* --- policy summary ---------------------------------------------------- */

export interface PolicySummary {
  kind: string;
  detailLines: string[];
}

export function summarizePolicy(decl: any): PolicySummary {
  if (!decl || typeof decl !== "object") return { kind: "(none)", detailLines: [] };
  const kind = String(decl.kind ?? "?");
  const lines: string[] = [];
  switch (kind) {
    case "owned_by":
      lines.push(`system: ${decl.system ?? "?"}`);
      break;
    case "state_machine": {
      const n = (decl.transitions || []).length;
      lines.push(`${n} transition${n === 1 ? "" : "s"}`);
      break;
    }
    case "set_by_key": {
      const id = Array.isArray(decl.identity) ? decl.identity.join(", ") : decl.identity ?? "?";
      lines.push(`id: (${id})`);
      lines.push(`a: ${decl.a_anchor ?? "?"}`);
      lines.push(`b: ${decl.b_anchor ?? "?"}`);
      if (decl.on_both_changed) lines.push(`both: ${decl.on_both_changed}`);
      if (decl.nested && Object.keys(decl.nested).length) {
        lines.push(`nested: ${Object.keys(decl.nested).join(", ")}`);
      }
      break;
    }
  }
  return { kind, detailLines: lines };
}

/* --- additive derivation hint ------------------------------------------ */

export function derivationHint(anc: Json, a: Json, b: Json, to: Json): string | null {
  const allNumeric = [anc, a, b, to].every((v) => typeof v === "number" && Number.isFinite(v));
  if (!allNumeric) return null;
  const ancN = anc as number;
  const aN = a as number;
  const bN = b as number;
  const toN = to as number;
  const dA = aN - ancN;
  const dB = bN - ancN;
  if (Math.abs(ancN + dA + dB - toN) > 1e-9) return null;
  if (dA === 0 && dB === 0) return null;
  const sign = (n: number) => (n >= 0 ? "+ " : "- ") + Math.abs(n);
  return `${ancN} ${sign(dA)} ${sign(dB)}`;
}

/* --- set_by_key element digest ----------------------------------------- */

export interface SbkRow {
  label: string;
  status: string;
  note: string;
  writtenMissing: boolean;
}

function compositeKeyLabel(elem: any, identity: any): string {
  if (!elem || typeof elem !== "object") return "(non-object)";
  const fields: string[] = Array.isArray(identity) ? identity : [identity];
  return fields
    .map((f) => `${f}=${elem[f] === undefined ? "∅" : JSON.stringify(elem[f])}`)
    .join(" / ");
}

export function setByKeyElementDigest(
  decl: any,
  anc: Json,
  a: Json,
  b: Json,
  written: Json,
): SbkRow[] {
  const identity = decl.identity;
  const aAnchor = decl.a_anchor;
  const bAnchor = decl.b_anchor;
  const ancArr: any[] = Array.isArray(anc) ? anc : [];
  const aArr: any[] = Array.isArray(a) ? a : [];
  const bArr: any[] = Array.isArray(b) ? b : [];
  const wArr: any[] = Array.isArray(written) ? written : [];

  const compKey = (e: any) => compositeKeyLabel(e, identity);
  const ancByA = new Map<string, string>();
  const ancByB = new Map<string, string>();
  for (const e of ancArr) {
    if (e && e[aAnchor] !== undefined) ancByA.set(String(e[aAnchor]), compKey(e));
    if (e && e[bAnchor] !== undefined) ancByB.set(String(e[bAnchor]), compKey(e));
  }
  const keyForSide = (e: any, anchor: string, anchorMap: Map<string, string>) => {
    const av = e && e[anchor];
    if (av !== undefined && anchorMap.has(String(av))) return anchorMap.get(String(av))!;
    return compKey(e);
  };
  const indexBy = (arr: any[], keyFn: (e: any) => string) => {
    const m = new Map<string, { elem: any; i: number }>();
    arr.forEach((e, i) => m.set(keyFn(e), { elem: e, i }));
    return m;
  };
  const ancIdx = indexBy(ancArr, compKey);
  const aIdx = indexBy(aArr, (e) => keyForSide(e, aAnchor, ancByA));
  const bIdx = indexBy(bArr, (e) => keyForSide(e, bAnchor, ancByB));
  const wIdx = indexBy(wArr, compKey);

  const allKeys = new Set([...ancIdx.keys(), ...aIdx.keys(), ...bIdx.keys(), ...wIdx.keys()]);
  const eq = (x: Json, y: Json) => JSON.stringify(x) === JSON.stringify(y);
  const rows: SbkRow[] = [];
  for (const key of [...allKeys].sort()) {
    const eA = ancIdx.get(key)?.elem;
    const e1 = aIdx.get(key)?.elem;
    const e2 = bIdx.get(key)?.elem;
    const eW = wIdx.get(key)?.elem;
    let status = "?";
    let note = "";
    if (e1 && compKey(e1) !== key) note += ` (A re-homed via ${aAnchor})`;
    if (e2 && compKey(e2) !== key) note += ` (B re-homed via ${bAnchor})`;
    if (eA && e1 && e2) {
      if (eq(e1, eA) && eq(e2, eA)) status = "unchanged";
      else if (eq(e1, e2)) status = "same-edit";
      else if (eq(e1, eA)) status = "changed-in-b";
      else if (eq(e2, eA)) status = "changed-in-a";
      else status = "changed-both";
    } else if (eA && e1 && !e2) status = "removed-in-b";
    else if (eA && !e1 && e2) status = "removed-in-a";
    else if (eA && !e1 && !e2) status = "removed-both";
    else if (!eA && e1 && e2) status = eq(e1, e2) ? "added-both" : "added-divergent";
    else if (!eA && e1) status = "added-in-a";
    else if (!eA && e2) status = "added-in-b";
    rows.push({ label: key, status, note, writtenMissing: !eW });
  }
  return rows;
}

/* --- pipeline stage body text ------------------------------------------ */

export function renderStageBody(stageKey: string, data: any): string {
  if (!data) return "(no data for this stage)";
  if (stageKey === "diff") {
    const d = data as DiffStage;
    const fmt = (arr: any[]) =>
      arr.length === 0
        ? "  (none)"
        : arr.map((x) => `  • ${x.path}: ${JSON.stringify(x.left)} → ${JSON.stringify(x.right)}`).join("\n");
    return [
      "A vs Ancestor:",
      fmt(d.a_vs_ancestor),
      "",
      "B vs Ancestor:",
      fmt(d.b_vs_ancestor),
      "",
      "A vs B:",
      fmt(d.a_vs_b),
    ].join("\n");
  }
  if (stageKey === "policy") {
    const p = data as PolicyStage;
    const parts: string[] = [];
    if (p.would_write !== null && p.would_write !== undefined) {
      parts.push("would_write:");
      parts.push(JSON.stringify(p.would_write, null, 2));
    } else {
      parts.push("would_write: (none — would escalate)");
    }
    if (p.conflicts && p.conflicts.length) {
      parts.push("");
      parts.push(`conflicts (${p.conflicts.length}):`);
      for (const c of p.conflicts) {
        parts.push(`  • ${c.path} [${c.class}] — ${c.reason}`);
      }
    }
    return parts.join("\n");
  }
  if (stageKey === "outcome") {
    const o = data as OutcomeDto;
    const lines = [`kind: ${o.kind}`];
    if (o.pushed_to && o.pushed_to.length) lines.push(`pushed_to: ${o.pushed_to.join(", ")}`);
    if (o.conflicts && o.conflicts.length) {
      lines.push("conflicts:");
      for (const c of o.conflicts) lines.push(`  • ${c.path} [${c.class}] — ${c.reason}`);
    }
    return lines.join("\n");
  }
  if (data.cif !== undefined) return JSON.stringify(data.cif, null, 2);
  return JSON.stringify(data, null, 2);
}

/* --- field changelog row generation ------------------------------------ */

export interface FieldChangelogRow {
  path: string;
  policy: PolicySummary;
  policyDecl: any;
  ancestor: { text: string; cell: RenderedCell };
  systemA: { text: string; cell: RenderedCell; isWinner: boolean };
  systemB: { text: string; cell: RenderedCell; isWinner: boolean };
  written: { text: string; cell: RenderedCell };
  winner: string;
  hint: string | null;
  sbkDigest: SbkRow[] | null;
}

export function buildFieldChangelogRows(
  ancestor: Record<string, Json>,
  a: Record<string, Json>,
  b: Record<string, Json>,
  written: Record<string, Json>,
  policies: Record<string, any>,
  aName: string,
  bName: string,
): FieldChangelogRow[] {
  const paths = new Set<string>([
    ...Object.keys(ancestor || {}),
    ...Object.keys(a || {}),
    ...Object.keys(b || {}),
    ...Object.keys(written || {}),
  ]);
  const rows: FieldChangelogRow[] = [];
  for (const path of [...paths].sort()) {
    const ancVal = ancestor?.[path];
    const aVal = a?.[path];
    const bVal = b?.[path];
    const toVal = written?.[path];
    if (stableEq(ancVal, toVal) && stableEq(aVal, toVal) && stableEq(bVal, toVal)) continue;

    const polDecl = policies?.[path];
    const ancText = fmtCell(ancVal);
    const aText = fmtCell(aVal);
    const bText = fmtCell(bVal);
    const toText = fmtCell(toVal);

    let winner = "—";
    if (stableEq(toVal, ancVal)) winner = "no-op";
    else if (stableEq(toVal, aVal) && stableEq(toVal, bVal)) winner = "both";
    else if (stableEq(toVal, aVal)) winner = aName;
    else if (stableEq(toVal, bVal)) winner = bName;
    else winner = "derived";

    const sbkDigest =
      polDecl && polDecl.kind === "set_by_key"
        ? setByKeyElementDigest(polDecl, ancVal, aVal, bVal, toVal)
        : null;

    rows.push({
      path,
      policy: summarizePolicy(polDecl),
      policyDecl: polDecl,
      ancestor: { text: ancText, cell: ancestorDiffCellLines(ancText, toText) },
      systemA: {
        text: aText,
        cell: diffCellLines(ancText, aText),
        isWinner: stableEq(aVal, toVal) && !stableEq(aVal, ancVal),
      },
      systemB: {
        text: bText,
        cell: diffCellLines(ancText, bText),
        isWinner: stableEq(bVal, toVal) && !stableEq(bVal, ancVal),
      },
      written: { text: toText, cell: diffCellLines(ancText, toText) },
      winner,
      hint: derivationHint(ancVal, aVal, bVal, toVal),
      sbkDigest: sbkDigest && sbkDigest.length ? sbkDigest : null,
    });
  }
  return rows;
}

/* --- StagesDto → RunContext (for FieldChangelog) ----------------------- */

export function buildContextFromStages(
  stages: StagesDto,
  systemAName: string,
  systemBName: string,
  policyPerField: Record<string, Json>,
): import("./types").RunContext {
  const wouldWrite = (stages.policy?.would_write ?? null) as Record<string, Json> | null;
  return {
    ancestor: (stages.diff?.ancestor_used ?? {}) as Record<string, Json>,
    cif_a: ((stages.transform_a as any)?.cif ?? {}) as Record<string, Json>,
    cif_b: ((stages.transform_b as any)?.cif ?? {}) as Record<string, Json>,
    would_write: wouldWrite,
    system_a_name: systemAName,
    system_b_name: systemBName,
    policy_per_field: policyPerField as Record<string, any>,
  };
}
