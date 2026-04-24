// Each entry = stages to activate together in one animation step.
const STEPS = [
  ["transform_a", "transform_b"],
  ["diff"],
  ["policy"],
  ["outcome"],
];
const STEP_MS = 500;

const SAMPLE = {
  system_a_name: "erp",
  system_b_name: "inv",
  // System A (erp, external) — each row carries `externalId` as its stable
  // per-system primary key. Here A has:
  //   • bumped qty on the (SKU-100, BTL) line
  //   • RENAMED uom of the second line from "CTN" to "BOX" — an identity
  //     field has mutated. Without the stable anchor, this would look like
  //     "one line removed, one added" to the merger.
  //   • added a brand-new (SKU-300, BTL) line
  system_a: {
    status: "closed",
    seqNumber: 42,
    supplier: { _id: "sup-1", name: "Acme Co." },
    price: 120,
    qty_recv: 6,
    lineItems: [
      { externalId: "A-L1", sku: "SKU-100", uom: "BTL", qty: 12 },
      { externalId: "A-L2", sku: "SKU-100", uom: "BOX", qty: 2 },
      { externalId: "A-L9", sku: "SKU-300", uom: "BTL", qty: 3 },
    ],
  },
  // System B (internal) — each row carries its stable `internalId`. B has
  // not touched the items, but the policy still needs to match B's rows to
  // the ancestor via `internalId`.
  system_b: {
    status: "closed",
    seqNumber: 42,
    supplier: { _id: "sup-1", name: "Acme Co." },
    price: 999,
    qty_recv: 7,
    items: [
      { internalId: "B-I1", sku: "SKU-100", uom: "BTL", qty: 10 },
      { internalId: "B-I2", sku: "SKU-100", uom: "CTN", qty: 2 },
    ],
  },
  schema: {
    cif_schema: {
      po_status: { type: "string", required: true },
      po_seq_number: { type: "number", required: true },
      supplier_id: { type: "string", required: true },
      price: { type: "number", required: true },
      qty_recv: { type: "number", required: true },
      // Cross-system array: declare the element shape so the library can
      // validate that the items `set_by_key` policy's anchor fields point
      // to real element fields. Each anchor is tagged with the side it
      // belongs to. Declared here, anchors become structural — the policy
      // validator rejects the config before the first cycle if anything
      // is misaligned.
      items: {
        type: "array",
        required: false,
        element: {
          externalId: { type: "string", anchor: "a" },
          internalId: { type: "string", anchor: "b" },
          sku: { type: "string" },
          uom: { type: "string" },
          qty: { type: "number" },
        },
      },
    },
    transformations: {
      erp: {
        po_status: { source_path: "status", type: "string" },
        po_seq_number: { source_path: "seqNumber", type: "number" },
        supplier_id: { source_path: "supplier._id", type: "string" },
        price: { source_path: "price", type: "number" },
        qty_recv: { source_path: "qty_recv", type: "number" },
        // Element-level source mappings — A carries externalId but no
        // internalId (that one only appears after a merge roundtrip).
        items: {
          source_path: "lineItems",
          type: "array",
          element: {
            externalId: { source_path: "externalId", type: "string" },
            sku: { source_path: "sku", type: "string" },
            uom: { source_path: "uom", type: "string" },
            qty: { source_path: "qty", type: "number" },
          },
        },
      },
      inv: {
        po_status: { source_path: "status", type: "string" },
        po_seq_number: { source_path: "seqNumber", type: "number" },
        supplier_id: { source_path: "supplier._id", type: "string" },
        price: { source_path: "price", type: "number" },
        qty_recv: { source_path: "qty_recv", type: "number" },
        // Mirror: B carries internalId, no externalId.
        items: {
          source_path: "items",
          type: "array",
          element: {
            internalId: { source_path: "internalId", type: "string" },
            sku: { source_path: "sku", type: "string" },
            uom: { source_path: "uom", type: "string" },
            qty: { source_path: "qty", type: "number" },
          },
        },
      },
    },
  },
  policy: {
    per_field: {
      price: { kind: "owned_by", system: "erp" },
      qty_recv: { kind: "additive" },
      po_status: {
        kind: "state_machine",
        transitions: [
          { from: "open", to: "closed" },
          { from: "open", to: "cancelled" },
        ],
      },
      // Cross-system item merge. Anchors are mandatory: real integrations
      // always have one side handing out immutable local IDs while business
      // fields mutate, and without anchors a rename corrupts three-way diffing.
      //   identity        : composite (sku, uom) — same sku on different UOMs
      //                     stay as distinct lines.
      //   a_anchor        : A's stable local ID (externalId). When A mutates
      //                     an identity field (e.g. renames uom), the row is
      //                     still re-homed to its ancestor via this anchor.
      //   b_anchor        : same for B (internalId).
      //   on_both_changed : "union" — matched rows merge fields, so both
      //                     `externalId` and `internalId` survive on one
      //                     record. Default is "escalate"; other modes are
      //                     "prefer_a" / "prefer_b".
      items: {
        kind: "set_by_key",
        identity: ["sku", "uom"],
        a_anchor: "externalId",
        b_anchor: "internalId",
        on_both_changed: "union",
      },
    },
  },
  // Ancestor represents the last-synced canonical state. Crucially, it
  // carries BOTH `externalId` and `internalId` for every line — that is the
  // cross-system ID map the anchor-based matcher reads. Without these, a
  // uom rename on either side would look like a remove+add.
  ancestor: {
    po_status: "open",
    po_seq_number: 42,
    supplier_id: "sup-1",
    price: 100,
    qty_recv: 5,
    items: [
      {
        sku: "SKU-100", uom: "BTL", qty: 10,
        externalId: "A-L1", internalId: "B-I1",
      },
      {
        sku: "SKU-100", uom: "CTN", qty: 2,
        externalId: "A-L2", internalId: "B-I2",
      },
    ],
  },
};

const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => document.querySelectorAll(sel);

function loadSample() {
  $("#system_a").value = JSON.stringify(SAMPLE.system_a, null, 2);
  $("#system_b").value = JSON.stringify(SAMPLE.system_b, null, 2);
  $("#schema").value = JSON.stringify(SAMPLE.schema, null, 2);
  $("#policy").value = JSON.stringify(SAMPLE.policy, null, 2);
  $("#ancestor").value = JSON.stringify(SAMPLE.ancestor, null, 2);
  $("#system_a_name").value = SAMPLE.system_a_name;
  $("#system_b_name").value = SAMPLE.system_b_name;
  updateNameLabels();
  resetStages();
}

function updateNameLabels() {
  $("#a-name-label").textContent = `(${$("#system_a_name").value || "system_a"})`;
  $("#b-name-label").textContent = `(${$("#system_b_name").value || "system_b"})`;
}

function parseJsonOrNull(raw, label, requireNonEmpty = true) {
  const s = raw.trim();
  if (!s) {
    if (requireNonEmpty) throw new Error(`${label} is empty`);
    return null;
  }
  try {
    return JSON.parse(s);
  } catch (e) {
    throw new Error(`${label}: ${e.message}`);
  }
}

function resetStages() {
  for (const s of $$(".stage")) {
    s.classList.remove("active", "done", "err");
    s.querySelector(".stage-body").textContent = "";
  }
  for (const a of $$(".arrow")) a.classList.remove("lit");
  const detail = $("#outcome-detail");
  detail.hidden = true;
  $("#outcome-conflicts").hidden = true;
  $("#outcome-merged").hidden = true;
  $("#outcome-changelog").hidden = true;
  $("#fc-body").textContent = "";
}

function setStatus(msg, tone = "") {
  const el = $("#status-msg");
  el.textContent = msg;
  el.className = "status-msg" + (tone ? " " + tone : "");
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

function renderStageBody(stageKey, data) {
  if (!data) return "(no data for this stage)";
  if (stageKey === "diff") {
    const fmt = (arr) => (arr.length === 0 ? "  (none)" : arr
      .map((d) => `  • ${d.path}: ${JSON.stringify(d.left)} → ${JSON.stringify(d.right)}`)
      .join("\n"));
    return [
      "A vs Ancestor:",
      fmt(data.a_vs_ancestor),
      "",
      "B vs Ancestor:",
      fmt(data.b_vs_ancestor),
      "",
      "A vs B:",
      fmt(data.a_vs_b),
    ].join("\n");
  }
  if (stageKey === "policy") {
    const parts = [];
    if (data.would_write) {
      parts.push("would_write:");
      parts.push(JSON.stringify(data.would_write, null, 2));
    } else {
      parts.push("would_write: (none — would escalate)");
    }
    if (data.conflicts && data.conflicts.length) {
      parts.push("");
      parts.push(`conflicts (${data.conflicts.length}):`);
      for (const c of data.conflicts) {
        parts.push(`  • ${c.path} [${c.class}] — ${c.reason}`);
      }
    }
    return parts.join("\n");
  }
  if (stageKey === "outcome") {
    const lines = [`kind: ${data.kind}`];
    if (data.pushed_to && data.pushed_to.length) {
      lines.push(`pushed_to: ${data.pushed_to.join(", ")}`);
    }
    if (data.conflicts && data.conflicts.length) {
      lines.push(`conflicts:`);
      for (const c of data.conflicts) {
        lines.push(`  • ${c.path} [${c.class}] — ${c.reason}`);
      }
    }
    return lines.join("\n");
  }
  if (data.cif !== undefined) return JSON.stringify(data.cif, null, 2);
  return JSON.stringify(data, null, 2);
}

function nextArrowAfter(el) {
  // Walk up from the stage to the stages container, then find the next
  // sibling arrow. If the stage lives in a .parallel-group, the group
  // itself is what's followed by an arrow.
  let node = el;
  while (node && node.parentElement && !node.parentElement.classList.contains("stages")) {
    node = node.parentElement;
  }
  if (!node) return null;
  const next = node.nextElementSibling;
  return next && next.classList.contains("arrow") ? next : null;
}

async function animate(stages, errorMsg) {
  for (const group of STEPS) {
    const els = group.map((key) =>
      document.querySelector(`.stage[data-stage="${key}"]`)
    );

    // Activate all stages in this step simultaneously.
    for (let i = 0; i < group.length; i++) {
      const key = group[i];
      const el = els[i];
      el.classList.add("active");
      el.querySelector(".stage-body").textContent = renderStageBody(key, stages[key]);
    }

    await sleep(STEP_MS);

    // Check for errors in any stage of this step.
    let failed = false;
    for (let i = 0; i < group.length; i++) {
      const key = group[i];
      const el = els[i];
      const body = stages[key];
      if (errorMsg && !body) {
        el.classList.remove("active");
        el.classList.add("err");
        el.querySelector(".stage-body").textContent = errorMsg;
        failed = true;
      }
    }
    if (failed) return;

    // Mark all done and light the single downstream arrow.
    for (const el of els) {
      el.classList.remove("active");
      el.classList.add("done");
    }
    const arrow = nextArrowAfter(els[0]);
    if (arrow) arrow.classList.add("lit");
  }
}

function makeEl(tag, opts = {}) {
  const el = document.createElement(tag);
  if (opts.className) el.className = opts.className;
  if (opts.text !== undefined) el.textContent = opts.text;
  return el;
}

function renderOutcomeDetail(outcome) {
  if (!outcome) return;
  const detail = $("#outcome-detail");
  detail.hidden = false;

  const summary = $("#outcome-summary");
  summary.className = "outcome-summary";
  summary.textContent = "";

  const strong = makeEl("strong");
  if (outcome.kind === "Synced") {
    summary.classList.add("synced");
    strong.textContent = "Synced.";
    summary.appendChild(strong);
    summary.appendChild(document.createTextNode(
      " Pushed to: " +
      (outcome.pushed_to.length ? outcome.pushed_to.join(", ") : "(nothing to push)")
    ));
  } else if (outcome.kind === "Escalated") {
    summary.classList.add("escalated");
    strong.textContent = "Escalated.";
    summary.appendChild(strong);
    summary.appendChild(document.createTextNode(
      ` ${outcome.conflicts.length} conflict(s) queued for review.`
    ));
  } else if (outcome.kind === "NoOp") {
    summary.classList.add("noop");
    strong.textContent = "No-op.";
    summary.appendChild(strong);
    summary.appendChild(document.createTextNode(
      " Neither side changed since ancestor."
    ));
  }

  // Conflicts
  const conflictsBox = $("#outcome-conflicts");
  const list = $("#conflicts-list");
  list.textContent = "";
  if (outcome.conflicts && outcome.conflicts.length) {
    conflictsBox.hidden = false;
    for (const c of outcome.conflicts) {
      const li = makeEl("li", { className: c.class });
      li.appendChild(makeEl("span", { className: "path", text: c.path }));
      li.appendChild(makeEl("span", { className: "class", text: c.class }));
      li.appendChild(makeEl("div", { className: "reason", text: c.reason }));
      list.appendChild(li);
    }
  } else {
    conflictsBox.hidden = true;
  }

  // Merged CIF — only meaningful for Synced.
  const mergedBox = $("#outcome-merged");
  const pre = $("#merged-pre");
  if (outcome.kind === "Synced" && window.__lastWouldWrite) {
    mergedBox.hidden = false;
    pre.textContent = JSON.stringify(window.__lastWouldWrite, null, 2);
  } else {
    mergedBox.hidden = true;
  }

  // Field changelog — per-field from→to record (only when we actually wrote).
  renderFieldChangelog(outcome);
}

function fmtCell(v) {
  if (v === undefined) return "—";
  if (v === null) return "null";
  if (typeof v === "string") return JSON.stringify(v);
  if (typeof v === "number" || typeof v === "boolean") return String(v);
  // Arrays / objects — pretty-print multi-line so the cell can wrap on the
  // structural newlines instead of getting squeezed into one huge blob.
  return JSON.stringify(v, null, 2);
}

/// Line-level diff (LCS) between two multi-line strings. Returns an array of
/// { type: "ctx" | "add" | "rm", text: "..." } entries that a renderer can
/// turn into git-style colored lines.
function lineDiff(fromStr, toStr) {
  const from = (fromStr === "—" || fromStr == null ? "" : String(fromStr)).split("\n");
  const to   = (toStr   === "—" || toStr   == null ? "" : String(toStr)  ).split("\n");
  const m = from.length, n = to.length;
  // LCS length table.
  const dp = Array.from({ length: m + 1 }, () => new Int32Array(n + 1));
  for (let i = m - 1; i >= 0; i--) {
    for (let j = n - 1; j >= 0; j--) {
      dp[i][j] = from[i] === to[j]
        ? dp[i + 1][j + 1] + 1
        : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const out = [];
  let i = 0, j = 0;
  while (i < m && j < n) {
    if (from[i] === to[j]) { out.push({ type: "ctx", text: to[j] }); i++; j++; }
    else if (dp[i + 1][j] >= dp[i][j + 1]) { out.push({ type: "rm", text: from[i] }); i++; }
    else { out.push({ type: "add", text: to[j] }); j++; }
  }
  while (i < m) out.push({ type: "rm", text: from[i++] });
  while (j < n) out.push({ type: "add", text: to[j++] });
  return out;
}

/// Render a table cell as a git-style diff against `fromText`. Used for A, B,
/// and Written columns. Returns a fragment of <span class="diff-line"> nodes.
function renderDiffCell(fromText, toText) {
  const frag = document.createDocumentFragment();
  const diff = lineDiff(fromText, toText);
  // If there are no actual changes, just render the value plain.
  const anyDelta = diff.some((d) => d.type !== "ctx");
  if (!anyDelta) {
    frag.appendChild(document.createTextNode(toText));
    return frag;
  }
  for (const entry of diff) {
    // Skip pure removal lines — they belong to the ancestor's column; the
    // target cell only shows context + additions. This keeps each column's
    // content aligned with its own value while still color-marking deltas.
    if (entry.type === "rm") continue;
    const line = document.createElement("div");
    line.className = "diff-line diff-" + entry.type;
    const prefix = document.createElement("span");
    prefix.className = "diff-prefix";
    prefix.textContent = entry.type === "add" ? "+ " : "  ";
    const body = document.createElement("span");
    body.className = "diff-body";
    body.textContent = entry.text;
    line.appendChild(prefix);
    line.appendChild(body);
    frag.appendChild(line);
  }
  return frag;
}

/// Render an ancestor cell as a git-style diff where lines REMOVED (present
/// in ancestor but not in `toText`) are marked with `-`. Used on the
/// Ancestor column so removed fields stand out visually.
function renderAncestorDiffCell(ancestorText, writtenText) {
  const frag = document.createDocumentFragment();
  const diff = lineDiff(ancestorText, writtenText);
  const anyDelta = diff.some((d) => d.type !== "ctx");
  if (!anyDelta) {
    frag.appendChild(document.createTextNode(ancestorText));
    return frag;
  }
  for (const entry of diff) {
    if (entry.type === "add") continue;
    const line = document.createElement("div");
    line.className = "diff-line diff-" + (entry.type === "rm" ? "rm" : "ctx");
    const prefix = document.createElement("span");
    prefix.className = "diff-prefix";
    prefix.textContent = entry.type === "rm" ? "- " : "  ";
    const body = document.createElement("span");
    body.className = "diff-body";
    body.textContent = entry.text;
    line.appendChild(prefix);
    line.appendChild(body);
    frag.appendChild(line);
  }
  return frag;
}

function stableEq(a, b) {
  if (a === b) return true;
  if (a === undefined || b === undefined) return false;
  return JSON.stringify(a) === JSON.stringify(b);
}

/// When a scalar-number field resolved to the additive CRDT formula
/// `ancestor + (A - ancestor) + (B - ancestor)`, return a compact breakdown
/// string (e.g. `"5 + 1 + 2"`) to show alongside the "derived" winner label.
/// Returns null when the pattern doesn't apply.
function derivationHint(anc, a, b, to) {
  const allNumeric = [anc, a, b, to].every((v) => typeof v === "number" && Number.isFinite(v));
  if (!allNumeric) return null;
  const deltaA = a - anc;
  const deltaB = b - anc;
  const expected = anc + deltaA + deltaB;
  if (Math.abs(expected - to) > 1e-9) return null;
  // Skip the trivial "no-op" degenerate.
  if (deltaA === 0 && deltaB === 0) return null;
  const sign = (n) => (n >= 0 ? "+ " : "- ") + Math.abs(n);
  return `${anc} ${sign(deltaA)} ${sign(deltaB)}`;
}

/// One-line (plus detail) summary of a policy declaration, used in the
/// outcome table's "Policy" column. Returns a DocumentFragment with the
/// kind highlighted and, for configurable kinds like `set_by_key`, an
/// indented detail block listing identity/anchors/on_both_changed.
function summarizePolicy(decl) {
  const frag = document.createDocumentFragment();
  if (!decl || typeof decl !== "object") {
    frag.appendChild(makeEl("span", { className: "pol-kind pol-none", text: "(none)" }));
    return frag;
  }
  const kind = decl.kind || "?";
  frag.appendChild(makeEl("span", { className: "pol-kind", text: kind }));

  const detail = document.createElement("div");
  detail.className = "pol-detail";
  const lines = [];
  switch (kind) {
    case "owned_by":
      lines.push(`system: ${decl.system || "?"}`);
      break;
    case "additive":
      break;
    case "append":
      break;
    case "state_machine": {
      const n = (decl.transitions || []).length;
      lines.push(`${n} transition${n === 1 ? "" : "s"}`);
      break;
    }
    case "set_by_key": {
      const id = Array.isArray(decl.identity)
        ? decl.identity.join(", ")
        : (decl.identity || "?");
      lines.push(`id: (${id})`);
      lines.push(`a: ${decl.a_anchor || "?"}`);
      lines.push(`b: ${decl.b_anchor || "?"}`);
      if (decl.on_both_changed) lines.push(`both: ${decl.on_both_changed}`);
      if (decl.nested && Object.keys(decl.nested).length) {
        lines.push(`nested: ${Object.keys(decl.nested).join(", ")}`);
      }
      break;
    }
    default:
      break;
  }
  for (const l of lines) {
    const div = document.createElement("div");
    div.textContent = l;
    detail.appendChild(div);
  }
  frag.appendChild(detail);
  return frag;
}

/// Build the composite key for one element per the policy's identity
/// declaration. Returns a printable "sku=X / uom=Y" style label.
function compositeKeyLabel(elem, identity) {
  if (!elem || typeof elem !== "object") return "(non-object)";
  const fields = Array.isArray(identity) ? identity : [identity];
  return fields
    .map((f) => `${f}=${elem[f] === undefined ? "∅" : JSON.stringify(elem[f])}`)
    .join(" / ");
}

/// For a set_by_key-governed array path, compute per-element status by
/// matching elements across ancestor / A / B / written using the declared
/// anchors (falling back to composite identity). Returns an array of
/// {label, status, note} entries.
function setByKeyElementDigest(decl, anc, a, b, written) {
  const identity = decl.identity;
  const aAnchor = decl.a_anchor;
  const bAnchor = decl.b_anchor;
  const ancArr = Array.isArray(anc) ? anc : [];
  const aArr = Array.isArray(a) ? a : [];
  const bArr = Array.isArray(b) ? b : [];
  const wArr = Array.isArray(written) ? written : [];

  // Build ancestor anchor → composite-key maps so we can rehome rows
  // whose identity fields mutated on one side.
  const compKey = (elem) => compositeKeyLabel(elem, identity);
  const ancByAAnchor = new Map();
  const ancByBAnchor = new Map();
  for (const e of ancArr) {
    if (e && e[aAnchor] !== undefined) ancByAAnchor.set(String(e[aAnchor]), compKey(e));
    if (e && e[bAnchor] !== undefined) ancByBAnchor.set(String(e[bAnchor]), compKey(e));
  }

  const keyForSide = (elem, anchorField, anchorMap) => {
    const av = elem && elem[anchorField];
    if (av !== undefined && anchorMap.has(String(av))) {
      return anchorMap.get(String(av));
    }
    return compKey(elem);
  };

  const indexBy = (arr, keyFn) => {
    const m = new Map();
    arr.forEach((e, i) => m.set(keyFn(e), { elem: e, i }));
    return m;
  };
  const ancIdx = indexBy(ancArr, (e) => compKey(e));
  const aIdx = indexBy(aArr, (e) => keyForSide(e, aAnchor, ancByAAnchor));
  const bIdx = indexBy(bArr, (e) => keyForSide(e, bAnchor, ancByBAnchor));
  const wIdx = indexBy(wArr, (e) => compKey(e));

  const allKeys = new Set([
    ...ancIdx.keys(), ...aIdx.keys(), ...bIdx.keys(), ...wIdx.keys(),
  ]);

  const eq = (x, y) => JSON.stringify(x) === JSON.stringify(y);
  const rows = [];
  for (const key of Array.from(allKeys).sort()) {
    const eA = ancIdx.get(key) && ancIdx.get(key).elem;
    const e1 = aIdx.get(key) && aIdx.get(key).elem;
    const e2 = bIdx.get(key) && bIdx.get(key).elem;
    const eW = wIdx.get(key) && wIdx.get(key).elem;

    let status;
    let note = "";
    // Tag re-homing: A or B element is using the ancestor's key even
    // though its own composite key differs (identity field mutated).
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
    else status = "?";

    rows.push({ label: key, status, note, writtenMissing: !eW });
  }
  return rows;
}

function renderFieldChangelog(outcome) {
  const box = $("#outcome-changelog");
  const tbody = $("#fc-body");
  tbody.textContent = "";

  const ctx = window.__lastContext || {};
  const written = ctx.would_write;
  if (outcome.kind !== "Synced" || !written) {
    box.hidden = true;
    return;
  }

  const ancestor = ctx.ancestor || {};
  const a = ctx.cif_a || {};
  const b = ctx.cif_b || {};
  const aName = ctx.system_a_name || "A";
  const bName = ctx.system_b_name || "B";
  const policies = ctx.policy_per_field || {};

  // Column headers carry the real system names. Shifted by one because
  // the Policy column now sits between Path and Ancestor.
  const head = document.querySelector("#outcome-changelog thead tr");
  head.children[3].textContent = `System A (${aName})`;
  head.children[4].textContent = `System B (${bName})`;

  // Union of every field path we've seen.
  const paths = new Set([
    ...Object.keys(ancestor),
    ...Object.keys(a),
    ...Object.keys(b),
    ...Object.keys(written),
  ]);
  if (paths.size === 0) {
    box.hidden = true;
    return;
  }

  let changedRows = 0;
  for (const path of [...paths].sort()) {
    const ancVal = ancestor[path];
    const aVal = a[path];
    const bVal = b[path];
    const toVal = written[path];

    // Skip paths where nothing effectively changed.
    if (stableEq(ancVal, toVal) && stableEq(aVal, toVal) && stableEq(bVal, toVal)) {
      continue;
    }
    changedRows++;

    const tr = makeEl("tr");
    tr.appendChild(makeEl("td", { className: "path", text: path }));

    // Policy column — show the declared policy kind + key args, so the
    // reader can correlate the winner with the rule that fired.
    const polDecl = policies[path];
    const polTd = makeEl("td", { className: "pol-cell" });
    polTd.appendChild(summarizePolicy(polDecl));
    tr.appendChild(polTd);

    const ancText = fmtCell(ancVal);
    const aText = fmtCell(aVal);
    const bText = fmtCell(bVal);
    const toText = fmtCell(toVal);

    // Ancestor column: mark the lines that got REMOVED in the final written
    // value (so deletions show up with `-` here).
    const ancTd = makeEl("td", { className: "val from" });
    ancTd.appendChild(renderAncestorDiffCell(ancText, toText));
    tr.appendChild(ancTd);

    // System A / System B columns: diff each side's value against ancestor,
    // so the cell shows context (unchanged) + additions relative to ancestor.
    const aTd = makeEl("td", { className: "val" });
    aTd.appendChild(renderDiffCell(ancText, aText));
    if (stableEq(aVal, toVal) && !stableEq(aVal, ancVal)) aTd.classList.add("winner");
    tr.appendChild(aTd);

    const bTd = makeEl("td", { className: "val" });
    bTd.appendChild(renderDiffCell(ancText, bText));
    if (stableEq(bVal, toVal) && !stableEq(bVal, ancVal)) bTd.classList.add("winner");
    tr.appendChild(bTd);

    // Written column: diff final written value against ancestor so the net
    // change across the sync is visible at a glance.
    const toTd = makeEl("td", { className: "val to" });
    toTd.appendChild(renderDiffCell(ancText, toText));

    // For set_by_key-governed arrays, attach a per-element digest so the
    // user sees which rows were added / changed / removed / re-homed
    // rather than having to eyeball the JSON blob.
    if (polDecl && polDecl.kind === "set_by_key") {
      const digest = setByKeyElementDigest(polDecl, ancVal, aVal, bVal, toVal);
      if (digest.length) {
        const detail = document.createElement("div");
        detail.className = "sbk-digest";
        for (const r of digest) {
          const line = document.createElement("div");
          line.className = "sbk-row sbk-" + r.status;
          line.textContent = `[${r.status}] ${r.label}${r.note}`;
          detail.appendChild(line);
        }
        toTd.appendChild(detail);
      }
    }

    tr.appendChild(toTd);

    let winner = "—";
    if (stableEq(toVal, ancVal)) {
      winner = "no-op";
    } else if (stableEq(toVal, aVal) && stableEq(toVal, bVal)) {
      winner = "both";
    } else if (stableEq(toVal, aVal)) {
      winner = aName;
    } else if (stableEq(toVal, bVal)) {
      winner = bName;
    } else {
      winner = "derived"; // e.g. additive sum
    }
    const wTd = makeEl("td", { className: `winner-cell winner-${winner}` });
    wTd.appendChild(document.createTextNode(winner));
    // For numeric additive-style derivations, annotate the arithmetic so
    // "derived" is self-explanatory instead of mysterious.
    const hint = derivationHint(ancVal, aVal, bVal, toVal);
    if (hint) {
      const sub = document.createElement("div");
      sub.className = "winner-hint";
      sub.textContent = hint;
      wTd.appendChild(sub);
    }
    tr.appendChild(wTd);

    tbody.appendChild(tr);
  }

  box.hidden = changedRows === 0;
}

async function runSync() {
  resetStages();
  const btn = $("#run-btn");
  btn.disabled = true;
  setStatus("Running…");

  let payload;
  try {
    payload = {
      system_a: parseJsonOrNull($("#system_a").value, "System A"),
      system_b: parseJsonOrNull($("#system_b").value, "System B"),
      schema: parseJsonOrNull($("#schema").value, "Schema"),
      policy: parseJsonOrNull($("#policy").value, "Policy"),
      ancestor: parseJsonOrNull($("#ancestor").value, "Ancestor", false),
      system_a_name: $("#system_a_name").value.trim() || "system_a",
      system_b_name: $("#system_b_name").value.trim() || "system_b",
    };
  } catch (e) {
    setStatus(e.message, "err");
    btn.disabled = false;
    return;
  }

  let res;
  try {
    res = await fetch("/sync", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
  } catch (e) {
    setStatus(`Network error: ${e.message}`, "err");
    btn.disabled = false;
    return;
  }

  if (!res.ok) {
    setStatus(`Server error: HTTP ${res.status}`, "err");
    btn.disabled = false;
    return;
  }

  const body = await res.json();
  const stages = body.stages || {};
  window.__lastWouldWrite = stages.policy ? stages.policy.would_write : null;
  window.__lastContext = {
    ancestor: stages.diff ? stages.diff.ancestor_used : null,
    cif_a: stages.transform_a ? stages.transform_a.cif : null,
    cif_b: stages.transform_b ? stages.transform_b.cif : null,
    would_write: stages.policy ? stages.policy.would_write : null,
    system_a_name: payload.system_a_name,
    system_b_name: payload.system_b_name,
    policy_per_field: (payload.policy && payload.policy.per_field) || {},
  };
  await animate(stages, body.error);

  if (body.error) {
    setStatus(body.error, "err");
  } else if (stages.outcome) {
    setStatus(`Done · ${stages.outcome.kind}`, "ok");
    renderOutcomeDetail(stages.outcome);
  } else {
    setStatus("Done (no outcome)", "");
  }

  btn.disabled = false;
}

document.addEventListener("DOMContentLoaded", () => {
  loadSample();
  $("#run-btn").addEventListener("click", runSync);
  $("#reset-btn").addEventListener("click", loadSample);
  $("#system_a_name").addEventListener("input", updateNameLabels);
  $("#system_b_name").addEventListener("input", updateNameLabels);
});
