// diff-fusion: TypeScript port — public entry point.

export {
  DiffFusion,
  type Conflict,
  type ConflictReport,
  type CompareResult,
} from "./drivers/facade.js";

export {
  SyncEngine,
  SyncEngineBuilder,
  type SyncOutcome,
  type FacadeConflict,
  type FacadePreview,
} from "./drivers/syncEngine.js";
