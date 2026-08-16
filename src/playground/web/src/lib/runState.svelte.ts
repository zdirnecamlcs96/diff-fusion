/// Top-level reactive state shared between DemoForm, WatchPanel, and
/// RunDialog. The dialog renders whatever is here; the two flows write
/// here when they have results to show.

import type { OutcomeDto, RunContext, StagesDto } from "./types";

interface RunState {
  dialogOpen: boolean;
  /** Stages to render inside the dialog. Filled by either the demo
   *  flow (after /sync resolves) or the watch flow (after replaying a
   *  cycle's events). */
  stages: StagesDto;
  /** Auxiliary context the field-changelog table needs. */
  context: RunContext | null;
  /** Optional title shown in the dialog header. */
  title: string;
  /** Optional subtitle (e.g. "demo · pasted JSON" or "watch · run-id / cycle #N"). */
  subtitle: string;
}

function emptyState(): RunState {
  return {
    dialogOpen: false,
    stages: {},
    context: null,
    title: "",
    subtitle: "",
  };
}

export const runState: RunState = $state(emptyState());

export function showRun(opts: {
  stages: StagesDto;
  context: RunContext | null;
  title: string;
  subtitle: string;
}): void {
  runState.stages = opts.stages;
  runState.context = opts.context;
  runState.title = opts.title;
  runState.subtitle = opts.subtitle;
  runState.dialogOpen = true;
}

export function closeRun(): void {
  runState.dialogOpen = false;
}

export function outcomeKind(stages: StagesDto): OutcomeDto["kind"] | null {
  return stages.outcome?.kind ?? null;
}
