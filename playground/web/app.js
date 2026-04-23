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
  system_a: {
    status: "closed",
    seqNumber: 42,
    version: "2",
    supplier: { _id: "sup-1", name: "Acme Co.", internal: "warehouse" },
    netSuite: { id: "NS-101", refNo: "ZPS-1" },
    price: 120,
    qty_recv: 6,
  },
  system_b: {
    status: "closed",
    seqNumber: 42,
    version: "2",
    supplier: { _id: "sup-1", name: "Acme Co.", internal: "warehouse" },
    netSuite: { id: "NS-101", refNo: "ZPS-1" },
    price: 999,
    qty_recv: 7,
  },
  schema: {
    cif_schema: {
      po_status: { type: "string", required: true },
      po_seq_number: { type: "number", required: true },
      supplier_id: { type: "string", required: true },
      price: { type: "number", required: true },
      qty_recv: { type: "number", required: true },
    },
    transformations: {
      erp: {
        po_status: { source_path: "status", type: "string" },
        po_seq_number: { source_path: "seqNumber", type: "number" },
        supplier_id: { source_path: "supplier._id", type: "string" },
        price: { source_path: "price", type: "number" },
        qty_recv: { source_path: "qty_recv", type: "number" },
      },
      inv: {
        po_status: { source_path: "status", type: "string" },
        po_seq_number: { source_path: "seqNumber", type: "number" },
        supplier_id: { source_path: "supplier._id", type: "string" },
        price: { source_path: "price", type: "number" },
        qty_recv: { source_path: "qty_recv", type: "number" },
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
    },
  },
  ancestor: {
    po_status: "open",
    po_seq_number: 42,
    supplier_id: "sup-1",
    price: 100,
    qty_recv: 5,
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
  window.__lastWouldWrite = body.stages && body.stages.policy ? body.stages.policy.would_write : null;
  await animate(body.stages || {}, body.error);

  if (body.error) {
    setStatus(body.error, "err");
  } else if (body.stages && body.stages.outcome) {
    setStatus(`Done · ${body.stages.outcome.kind}`, "ok");
    renderOutcomeDetail(body.stages.outcome);
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
