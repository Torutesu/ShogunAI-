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

  it("rejects a cancelled subscription's token with 402 even before exp", async () => {
    const rig = await makeRig();
    const cancelled = await signLicense(rig.keys.privateKey, { status: "canceled" });
    const res = await postBatch(rig, sampleBody, cancelled);
    expect(res.status).toBe(402);
    expect(rig.gateway.createCalls).toHaveLength(0);
  });
});

describe("GET /v1/batch/:id ownership (§4.1)", () => {
  it("404s another licence's batch id and never touches the gateway", async () => {
    const rig = await makeRig();
    // lic_test_1 creates the batch…
    expect((await postBatch(rig, sampleBody)).status).toBe(202);
    // …and a different (validly licensed!) user tries to read it back.
    const other = await signLicense(rig.keys.privateKey, { sub: "lic_other" });
    const res = await rig.app.request("/v1/batch/rb_fake_1", {
      headers: { authorization: `Bearer ${other}` },
    });
    expect(res.status).toBe(404);
    expect(rig.gateway.getCalls).toHaveLength(0);
    expect(rig.gateway.resultsCalls).toHaveLength(0);
  });

  it("404s a batch id this relay never issued (no oracle for probing)", async () => {
    const rig = await makeRig();
    const res = await rig.app.request("/v1/batch/msgbatch_guessed", {
      headers: { authorization: `Bearer ${rig.token}` },
    });
    expect(res.status).toBe(404);
    expect(rig.gateway.getCalls).toHaveLength(0);
  });

  it("still serves the creator its own batch", async () => {
    const rig = await makeRig();
    expect((await postBatch(rig, sampleBody)).status).toBe(202);
    const res = await rig.app.request("/v1/batch/rb_fake_1", {
      headers: { authorization: `Bearer ${rig.token}` },
    });
    expect(res.status).toBe(200);
    expect(rig.gateway.getCalls).toEqual(["rb_fake_1"]);
  });
});
