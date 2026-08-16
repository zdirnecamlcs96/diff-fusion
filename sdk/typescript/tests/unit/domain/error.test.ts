import { describe, expect, it } from "vitest";
import { SyncError } from "../../../src/domain/error.js";

describe("SyncError", () => {
  it("display includes context", () => {
    const err = SyncError.staleWrite({
      system: "sap",
      expected: "v3",
      message: "version moved",
    });
    const s = err.message;
    expect(s).toContain("sap");
    expect(s).toContain("v3");
  });

  it("transient carries the message in .message", () => {
    const err = SyncError.transient("timeout");
    expect(err.kind).toBe("Transient");
    expect(err.message).toContain("timeout");
  });

  it("staleWrite accepts an absent expected version", () => {
    const err = SyncError.staleWrite({
      system: "sap",
      message: "version moved",
    });
    expect(err.kind).toBe("StaleWrite");
    expect(err.expected).toBeUndefined();
  });

  it("conflict exposes paths", () => {
    const err = SyncError.conflict({ paths: ["price", "qty"] });
    expect(err.kind).toBe("Conflict");
    expect(err.paths).toEqual(["price", "qty"]);
    expect(err.message).toContain("price");
    expect(err.message).toContain("qty");
  });

  it("is an instance of Error and SyncError", () => {
    const err = SyncError.transient("x");
    expect(err).toBeInstanceOf(Error);
    expect(err).toBeInstanceOf(SyncError);
    expect(err.name).toBe("SyncError");
  });

  it("has a stack trace", () => {
    const err = SyncError.transient("x");
    expect(typeof err.stack).toBe("string");
    expect(err.stack).toBeTruthy();
  });
});
