// Pack G: DrainReport typed shape.
//
// `admin.drainVectorProjection` previously returned
// `Record<string, unknown>`, which forced callers to cast fields at the use
// site. It now returns a typed `DrainReport` with stable numeric fields.
// This test pins both (a) the shape at the type level (compiles only if the
// fields exist with the expected names/types), and (b) the wire→object
// adapter produces zeros for a missing/empty input.

import { describe, expect, it } from "vitest";

import type { DrainReport } from "../src/index.js";

// Internal helper is also imported for a round-trip assertion.
import { drainReportFromWire } from "../src/types.js";

describe("DrainReport typed shape (Pack G)", () => {
  it("has the five numeric fields mirroring the Rust struct", () => {
    // Construct a literal; TypeScript will reject at compile time if any
    // field is missing / misspelled / wrong type.
    const r: DrainReport = {
      incremental_processed: 0,
      backfill_processed: 0,
      failed: 0,
      discarded_stale: 0,
      embedder_unavailable_ticks: 0,
    };
    // Reflection check to catch accidental drift in the field list.
    const keys = Object.keys(r).sort();
    expect(keys).toEqual([
      "backfill_processed",
      "discarded_stale",
      "embedder_unavailable_ticks",
      "failed",
      "incremental_processed",
    ]);
    for (const [, value] of Object.entries(r)) {
      expect(typeof value).toBe("number");
    }
  });

  it("wire adapter fills missing numeric fields with 0", () => {
    const r = drainReportFromWire({});
    expect(r).toEqual({
      incremental_processed: 0,
      backfill_processed: 0,
      failed: 0,
      discarded_stale: 0,
      embedder_unavailable_ticks: 0,
    });
  });

  it("wire adapter preserves all five numeric fields from a wire payload", () => {
    const r = drainReportFromWire({
      incremental_processed: 3,
      backfill_processed: 17,
      failed: 1,
      discarded_stale: 2,
      embedder_unavailable_ticks: 1,
    });
    expect(r.incremental_processed).toBe(3);
    expect(r.backfill_processed).toBe(17);
    expect(r.failed).toBe(1);
    expect(r.discarded_stale).toBe(2);
    expect(r.embedder_unavailable_ticks).toBe(1);
  });
});
