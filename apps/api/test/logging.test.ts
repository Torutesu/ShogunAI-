/** §3.2: the logger never sees a body. It logs method/path/status/duration and numeric counters
 * only — chunk content and bearer tokens are structurally out of reach. */
import { describe, expect, it } from "vitest";

import { makeRig, postBatch, sampleBody } from "./helpers.js";

describe("redacting logger", () => {
  it("logs one line per request with method, path and status — never the body", async () => {
    const rig = await makeRig();
    await postBatch(rig, sampleBody);
    expect(rig.logLines).toHaveLength(1);
    const line = rig.logLines[0] ?? "";
    expect(line).toMatch(/^POST \/v1\/batch 202 \d+ms/);
    expect(line).toContain("chunks=2");
    expect(line).toContain("license=lic_test_1");
    expect(line).not.toContain("TOP-SECRET");
  });

  it("never logs the bearer token, even on auth failures", async () => {
    const rig = await makeRig();
    await postBatch(rig, sampleBody, "not-a-real-token-value");
    const all = rig.logLines.join("\n");
    expect(all).toContain("POST /v1/batch 401");
    expect(all).not.toContain("not-a-real-token-value");
  });

  it("logs GET result fetches without any result content", async () => {
    const rig = await makeRig();
    // The ownership check reads the store before the gateway — register rb_1 as this licence's.
    await rig.usage.attachBatch("rb_1", "lic_test_1", "2026-08-13", 1);
    rig.gateway.status = {
      id: "rb_1",
      processing_status: "ended",
      request_counts: { processing: 0, succeeded: 1, errored: 0, canceled: 0, expired: 0 },
    };
    rig.gateway.resultLines = [
      JSON.stringify({
        custom_id: "1",
        result: { type: "succeeded", message: { content: [{ type: "text", text: "SECRET-RESULT" }] } },
      }),
    ];
    const res = await rig.app.request("/v1/batch/rb_1", {
      headers: { authorization: `Bearer ${rig.token}` },
    });
    await res.text(); // drain the stream so the request fully completes
    const all = rig.logLines.join("\n");
    expect(all).toContain("GET /v1/batch/rb_1 200");
    expect(all).not.toContain("SECRET-RESULT");
  });
});
