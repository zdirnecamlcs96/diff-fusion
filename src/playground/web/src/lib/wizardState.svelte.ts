/// Reactive form state for the stepper-driven New Test flow. All step
/// components read and write these fields directly. Lives at module
/// scope so the Dashboard can prime it (e.g. "load capture into form")
/// before switching views.

import type { Capture, TestRecord } from "./types";
import { SAMPLE } from "./samples";
import { identityTransformerFor, schemaFromCapture } from "./captureLoad";

export const STEPS = [
  { key: "schema", label: "Schema" },
  { key: "policy", label: "Policy" },
  { key: "transformers", label: "Transformers" },
  { key: "data", label: "Data" },
  { key: "review", label: "Review" },
] as const;

export type StepKey = (typeof STEPS)[number]["key"];

interface WizardState {
  /** Stable id for this test in the backend's TestStore. Generated on
   *  first save so re-runs and outcome patches address the same record. */
  testId: string | null;
  systemA: string;
  systemB: string;
  cifSchema: string;
  policy: string;
  ancestor: string;
  transformerA: string;
  transformerB: string;
  systemAName: string;
  systemBName: string;
  /** Display name shown on the dialog + dashboard heading. */
  testName: string;
  /** Index into STEPS. */
  currentStep: number;
  /** Inline status line below the stepper / on the review step. */
  status: string;
  statusTone: "" | "ok" | "err";
  running: boolean;
  suggesting: boolean;
}

function blank(): WizardState {
  return {
    testId: null,
    systemA: "",
    systemB: "",
    cifSchema: "",
    policy: "",
    ancestor: "",
    transformerA: "",
    transformerB: "",
    systemAName: "erp",
    systemBName: "inv",
    testName: "Untitled test",
    currentStep: 0,
    status: "",
    statusTone: "",
    running: false,
    suggesting: false,
  };
}

export const wizard: WizardState = $state(blank());

export function setStatus(msg: string, tone: "" | "ok" | "err" = ""): void {
  wizard.status = msg;
  wizard.statusTone = tone;
}

export function gotoStep(i: number): void {
  if (i < 0) return;
  if (i >= STEPS.length) return;
  wizard.currentStep = i;
}

export function nextStep(): void {
  gotoStep(wizard.currentStep + 1);
}

export function prevStep(): void {
  gotoStep(wizard.currentStep - 1);
}

export function isLastStep(): boolean {
  return wizard.currentStep === STEPS.length - 1;
}

/** Reset to a blank form on step 0. Called when the user clicks
 *  "+ New Test" from the dashboard. */
export function resetWizard(): void {
  Object.assign(wizard, blank());
}

/** Pre-fill from the bundled SAMPLE (the legacy "Reset to sample"). */
export function initSample(): void {
  Object.assign(wizard, blank());
  wizard.systemA = JSON.stringify(SAMPLE.system_a, null, 2);
  wizard.systemB = JSON.stringify(SAMPLE.system_b, null, 2);
  wizard.cifSchema = JSON.stringify({ cif_schema: SAMPLE.schema.cif_schema }, null, 2);
  wizard.transformerA = JSON.stringify(SAMPLE.schema.transformations[SAMPLE.system_a_name], null, 2);
  wizard.transformerB = JSON.stringify(SAMPLE.schema.transformations[SAMPLE.system_b_name], null, 2);
  wizard.policy = JSON.stringify(SAMPLE.policy, null, 2);
  wizard.ancestor = JSON.stringify(SAMPLE.ancestor, null, 2);
  wizard.systemAName = SAMPLE.system_a_name;
  wizard.systemBName = SAMPLE.system_b_name;
  wizard.testName = "Sample · PO fulfillment";
  setStatus("Loaded sample", "ok");
}

/** Pre-fill from a saved capture: post-transform CIF on each side, plus
 *  a synthesised schema and identity transformers. Policy and ancestor
 *  are left blank — the user owns those choices. */
export function initFromCapture(captureId: string, c: Capture): void {
  Object.assign(wizard, blank());
  wizard.systemA = JSON.stringify(c.side_a.canonical_view ?? {}, null, 2);
  wizard.systemB = JSON.stringify(c.side_b.canonical_view ?? {}, null, 2);
  wizard.systemAName = c.side_a.system;
  wizard.systemBName = c.side_b.system;
  wizard.cifSchema = JSON.stringify(schemaFromCapture(c), null, 2);
  wizard.transformerA = JSON.stringify(identityTransformerFor(c.side_a.canonical_view), null, 2);
  wizard.transformerB = JSON.stringify(identityTransformerFor(c.side_b.canonical_view), null, 2);
  wizard.testName = `Capture · ${captureId}`;
  setStatus(`Loaded capture ${captureId}`, "ok");
}

/** Pre-fill from a saved test: textareas come back verbatim. The testId
 *  is preserved so re-running updates the same record (last_outcome
 *  reflects the latest run). */
export function initFromTest(testId: string, t: TestRecord): void {
  Object.assign(wizard, blank());
  wizard.testId = testId;
  wizard.systemA = t.system_a;
  wizard.systemB = t.system_b;
  wizard.cifSchema = t.cif_schema;
  wizard.policy = t.policy;
  wizard.ancestor = t.ancestor;
  wizard.transformerA = t.transformer_a;
  wizard.transformerB = t.transformer_b;
  wizard.systemAName = t.system_a_name;
  wizard.systemBName = t.system_b_name;
  wizard.testName = t.name;
  setStatus(`Loaded test ${t.name}`, "ok");
}

/** Snapshot the current wizard state as a TestRecord ready to POST. */
export function toTestRecord(): TestRecord {
  return {
    name: wizard.testName?.trim() || "Untitled test",
    cif_schema: wizard.cifSchema,
    policy: wizard.policy,
    transformer_a: wizard.transformerA,
    transformer_b: wizard.transformerB,
    system_a: wizard.systemA,
    system_b: wizard.systemB,
    ancestor: wizard.ancestor,
    system_a_name: wizard.systemAName,
    system_b_name: wizard.systemBName,
    last_outcome: null,
  };
}
