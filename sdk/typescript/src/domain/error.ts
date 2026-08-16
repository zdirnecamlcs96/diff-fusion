/**
 * Error categories for the sync cycle.
 *
 * Every error that flows through the orchestrator falls into exactly one of
 * three categories. Each category drives a different recovery path:
 *
 * - `Transient` — retry with backoff (network, rate limit, 5xx).
 * - `StaleWrite` — restart the cycle; another actor moved first.
 * - `Conflict` — the resolver cannot decide; route to escalation.
 *
 * The categories are the interface. Never construct a bare error — use a
 * static constructor.
 */

export type SyncErrorKind = "Transient" | "StaleWrite" | "Conflict";

interface TransientFields {
  kind: "Transient";
}

interface StaleWriteFields {
  kind: "StaleWrite";
  system: string;
  expected: string | undefined;
}

interface ConflictFields {
  kind: "Conflict";
  paths: readonly string[];
}

type Fields = TransientFields | StaleWriteFields | ConflictFields;

export class SyncError extends Error {
  readonly kind: SyncErrorKind;
  readonly system?: string;
  readonly expected?: string;
  readonly paths?: readonly string[];

  private constructor(message: string, fields: Fields) {
    super(message);
    this.name = "SyncError";
    this.kind = fields.kind;
    if (fields.kind === "StaleWrite") {
      this.system = fields.system;
      if (fields.expected !== undefined) {
        this.expected = fields.expected;
      }
    } else if (fields.kind === "Conflict") {
      this.paths = fields.paths;
    }
    if (typeof (Error as unknown as { captureStackTrace?: unknown }).captureStackTrace === "function") {
      (Error as unknown as { captureStackTrace: (t: object, c?: Function) => void }).captureStackTrace(
        this,
        SyncError,
      );
    }
  }

  static transient(msg: string): SyncError {
    return new SyncError(`transient failure: ${msg}`, { kind: "Transient" });
  }

  static staleWrite(args: { system: string; message: string; expected?: string }): SyncError {
    const expectedDisplay = args.expected === undefined ? "None" : `Some(${JSON.stringify(args.expected)})`;
    const msg = `stale write: ${args.message} (system=${args.system}, expected_version=${expectedDisplay})`;
    return new SyncError(msg, {
      kind: "StaleWrite",
      system: args.system,
      expected: args.expected,
    });
  }

  static conflict(args: { paths: readonly string[] }): SyncError {
    const list = args.paths.map((p) => JSON.stringify(p)).join(", ");
    const msg = `unresolved conflict(s): [${list}]`;
    return new SyncError(msg, { kind: "Conflict", paths: args.paths });
  }
}
