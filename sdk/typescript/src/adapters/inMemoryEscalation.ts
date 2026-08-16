/**
 * In-memory `EscalationQueue` — reference impl used by tests and the default
 * `SyncEngine` configuration.
 *
 * Not durable. Production deployments implement the interface against a
 * durable store (Postgres, SQS, etc.) so items survive a restart.
 *
 * Node's event loop handles ordering; no lock needed around the backing
 * array.
 */

import {
  type EscalationItem,
  type EscalationQueue,
} from "../ports/escalation.js";

export class InMemoryEscalationQueue implements EscalationQueue {
  private readonly items: EscalationItem[] = [];

  async push(item: EscalationItem): Promise<void> {
    this.items.push(item);
  }

  async len(): Promise<number> {
    return this.items.length;
  }

  async isEmpty(): Promise<boolean> {
    return this.items.length === 0;
  }

  /** Snapshot the queue — tests use this to assert what was escalated. */
  snapshot(): readonly EscalationItem[] {
    // Defensive copy: callers can't mutate our backing array.
    return this.items.slice();
  }
}
