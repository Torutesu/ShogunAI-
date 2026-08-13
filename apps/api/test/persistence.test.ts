/** §3.2 / NFR-PRV-04: the relay stores no chunk content. After a full submit→poll→results
 * round trip, everything the relay persisted (usage ledger) or emitted (log lines) must be free
 * of both the request chunks and the model's results — the only durable trace is aggregates. */
import { describe, expect, it } from "vitest";

import { InMemoryUsageStore } from "../src/usage.js";
import { makeRig, postBatch } from "./helpers.js";

const CHUNK_MARKER = "CHUNK-MARKER-9f3a";
const RESULT_MARKER = "RESULT-MARKER-7c1d";

describe("no persistence of chunk content", () => {
  it("keeps chunks and results out of the usage store and the logs across a full round trip", async () => {
    const rig = await makeRig();

    const submit = await postBatch(rig, {
      purpose: "consolidation",
      model_class: "classify",
      items: [{ custom_id: "42", chunk: `classify this: ${CHUNK_MARKER}` }],
    });
    expect(submit.status).toBe(202);

    rig.gateway.status = {
      id: "rb_fake_1",
      processing_status: "ended",
      request_counts: { processing: 0, succeeded: 1, errored: 0, canceled: 0, expired: 0 },
    };
    rig.gateway.resultLines = [
      JSON.stringify({
        custom_id: "42",
        result: { type: "succeeded", message: { content: [{ type: "text", text: RESULT_MARKER }] } },
      }),
    ];
    const results = await rig.app.request("/v1/batch/rb_fake_1", {
      headers: { authorization: `Bearer ${rig.token}` },
    });
    const passedThrough = await results.text();
    // The content PASSES THROUGH to the device…
    expect(passedThrough).toContain(RESULT_MARKER);

    // …but nothing the relay kept contains either marker.
    const persisted = JSON.stringify(rig.usage.records) + JSON.stringify(rig.usage);
    const logged = rig.logLines.join("\n");
    for (const marker of [CHUNK_MARKER, RESULT_MARKER]) {
      expect(persisted).not.toContain(marker);
      expect(logged).not.toContain(marker);
    }
    // What IS persisted is exactly the billing aggregate.
    expect(rig.usage.records).toEqual([
      { date: expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/), licenseId: "lic_test_1", chunks: 1, batchId: "rb_fake_1" },
    ]);
  });

  it("the UsageStore interface cannot carry content: only counters land in the ledger", async () => {
    const store = new InMemoryUsageStore();
    expect(await store.tryReserve("lic_x", "2026-08-09", 3, 100)).toBe("ok");
    expect(await store.tryReserve("lic_x", "2026-08-09", 2, 100)).toBe("ok");
    await store.attachBatch("rb_1", "lic_x", "2026-08-09", 3);
    await store.attachBatch("rb_2", "lic_x", "2026-08-09", 2);
    expect(await store.usedOn("lic_x", "2026-08-09")).toBe(5);
    expect(await store.usedOn("lic_x", "2026-08-10")).toBe(0);
    expect(await store.usedOn("lic_y", "2026-08-09")).toBe(0);
  });
});
