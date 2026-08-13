/** §2.2/§4.5: the daily chunk cap — the reason the relay exists. Enforced server-side, per
 * license, per UTC day; a capped submission is refused with 429 and never reaches Anthropic. */
import { describe, expect, it } from "vitest";

import { makeRig, postBatch } from "./helpers.js";

function bodyWithChunks(n: number): unknown {
  return {
    purpose: "consolidation",
    model_class: "classify",
    items: Array.from({ length: n }, (_, i) => ({ custom_id: String(i), chunk: `c${i}` })),
  };
}

describe("daily chunk cap", () => {
  it("accepts submissions up to the cap and refuses the one that would cross it", async () => {
    const rig = await makeRig({ dailyChunkCap: 10 });

    expect((await postBatch(rig, bodyWithChunks(8))).status).toBe(202);
    // 8 used + 3 more would cross 10 → 429, and the gateway is not called again.
    const over = await postBatch(rig, bodyWithChunks(3));
    expect(over.status).toBe(429);
    expect(rig.gateway.createCalls).toHaveLength(1);
    // Exactly at the cap is still allowed.
    expect((await postBatch(rig, bodyWithChunks(2))).status).toBe(202);
  });

  it("records usage only for accepted submissions", async () => {
    const rig = await makeRig({ dailyChunkCap: 5 });
    await postBatch(rig, bodyWithChunks(4));
    await postBatch(rig, bodyWithChunks(4)); // refused
    expect(rig.usage.records).toHaveLength(1);
    expect(rig.usage.records[0]?.chunks).toBe(4);
  });

  it("meters per license, not globally", async () => {
    const rig = await makeRig({ dailyChunkCap: 5 });
    const { signLicense } = await import("./helpers.js");
    const other = await signLicense(rig.keys.privateKey, { sub: "lic_other" });
    expect((await postBatch(rig, bodyWithChunks(5))).status).toBe(202);
    // The first license is at its cap; a different license still has headroom.
    expect((await postBatch(rig, bodyWithChunks(5))).status).toBe(429);
    expect((await postBatch(rig, bodyWithChunks(5), other)).status).toBe(202);
  });

  it("resets on the next UTC day", async () => {
    let today = new Date("2026-08-09T23:00:00Z");
    const rig = await makeRig({ dailyChunkCap: 5, now: () => today });
    expect((await postBatch(rig, bodyWithChunks(5))).status).toBe(202);
    expect((await postBatch(rig, bodyWithChunks(1))).status).toBe(429);
    today = new Date("2026-08-10T01:00:00Z");
    expect((await postBatch(rig, bodyWithChunks(5))).status).toBe(202);
  });
});
