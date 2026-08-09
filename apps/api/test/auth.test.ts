/** §4.1/§4.5: license verification — valid, expired, wrong key, missing, plan-gated. */
import { describe, expect, it } from "vitest";

import { makeKeys, makeRig, postBatch, sampleBody, signLicense } from "./helpers.js";

describe("POST /v1/batch auth", () => {
  it("accepts a valid license and returns 202 with the batch id", async () => {
    const rig = await makeRig();
    const res = await postBatch(rig, sampleBody);
    expect(res.status).toBe(202);
    const body = (await res.json()) as { batch_id: string; accepted: number };
    expect(body.batch_id).toBe("rb_fake_1");
    expect(body.accepted).toBe(2);
  });

  it("rejects an expired token with 401 and never reaches the gateway", async () => {
    const rig = await makeRig();
    const expired = await signLicense(rig.keys.privateKey, { expiresIn: "-1h" });
    const res = await postBatch(rig, sampleBody, expired);
    expect(res.status).toBe(401);
    expect(rig.gateway.createCalls).toHaveLength(0);
  });

  it("rejects a token signed with a different key with 401", async () => {
    const rig = await makeRig();
    const stranger = await makeKeys();
    const forged = await signLicense(stranger.privateKey);
    const res = await postBatch(rig, sampleBody, forged);
    expect(res.status).toBe(401);
    expect(rig.gateway.createCalls).toHaveLength(0);
  });

  it("rejects a missing Authorization header with 401", async () => {
    const rig = await makeRig();
    const res = await rig.app.request("/v1/batch", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(sampleBody),
    });
    expect(res.status).toBe(401);
  });

  it("rejects a plan outside the Batch lane with 402 (§4.5)", async () => {
    const rig = await makeRig();
    const freeloader = await signLicense(rig.keys.privateKey, { plan: "expired-trial" });
    const res = await postBatch(rig, sampleBody, freeloader);
    expect(res.status).toBe(402);
    expect(rig.gateway.createCalls).toHaveLength(0);
  });

  it("guards GET /v1/batch/:id with the same verification", async () => {
    const rig = await makeRig();
    const res = await rig.app.request("/v1/batch/rb_1");
    expect(res.status).toBe(401);
    expect(rig.gateway.getCalls).toHaveLength(0);
  });
});
