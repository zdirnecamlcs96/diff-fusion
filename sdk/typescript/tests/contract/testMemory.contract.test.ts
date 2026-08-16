/**
 * Driver: run the shared SystemPort contract suite against
 * {@link TestMemoryAdapter}.
 *
 * The reference in-memory adapter is the canary for the suite itself — if a
 * regression breaks the contract here, every other adapter's contract run
 * becomes untrustworthy.
 *
 * Ports Rust `test_memory_adapter_passes_contract` + the `_under_other_name`
 * twin, which exercises the suite under a second `systemType` label to prove
 * no check secretly hard-codes the identifier.
 */

import { TestMemoryAdapter } from "../../src/adapters/testMemory.js";
import { runContractSuite } from "./systemPortContract.js";

runContractSuite(
  "TestMemoryAdapter(contract_sys_a)",
  () => new TestMemoryAdapter("contract_sys_a"),
);

runContractSuite(
  "TestMemoryAdapter(contract_sys_b)",
  () => new TestMemoryAdapter("contract_sys_b"),
);
