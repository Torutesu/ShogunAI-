/** §4.2–§4.4: the forward is a passthrough for custom_id and chunk content, and the model is a
 * server decision the client cannot influence. */
import { describe, expect, it } from "vitest";

import { MODEL_BY_CLASS } from "../src/models.js";
import { makeRig, postBatch, sampleBody } from "./helpers.js";

describe("forwarding", () => {
  it("preserves custom_ids and chunks, and picks the model server-side", async () => {
    const rig = await makeRig();
    const res = await postBatch(rig, sampleBody);
    expect(res.status).toBe(202);

    const forwarded = rig.gateway.createCalls[0];
    expect(forwarded).toBeDefined();
    expect(forwarded?.map((r) => r.custom_id)).toEqual(["1234", "5678"]);
    expect(forwarded?.[0]?.params.messages[0]?.content).toBe("TOP-SECRET-CHUNK-ALPHA");
    expect(forwarded?.[0]?.params.model).toBe(MODEL_BY_CLASS.classify);
  });

  it("ignores a client-sent model id — the intent is all the device gets to say (§4.4)", async () => {
    const rig = await makeRig();
    const res = await postBatch(rig, {
      ...sampleBody,
      model: "claude-opus-4-6", // not part of the contract; must not be read
      items: [{ custom_id: "1", chunk: "c", model: "claude-opus-4-6" }],
    });
    expect(res.status).toBe(202);
    const forwarded = rig.gateway.createCalls[0];
    expect(forwarded?.[0]?.params.model).toBe(MODEL_BY_CLASS.classify);
    expect(JSON.stringify(forwarded)).not.toContain("opus");
  });

  it("rejects an unknown model_class with 400 before touching the gateway", async () => {
    const rig = await makeRig();
    const res = await postBatch(rig, { ...sampleBody, model_class: "claude-opus-4-6" });
    expect(res.status).toBe(400);
    expect(rig.gateway.createCalls).toHaveLength(0);
  });

  it("passes an in-progress status through as {status, completed, total} (§4.3)", async () => {
    const rig = await makeRig();
    rig.gateway.status = {
      id: "rb_1",
      processing_status: "in_progress",
      request_counts: { processing: 512, succeeded: 280, errored: 20, canceled: 0, expired: 0 },
    };
    const res = await rig.app.request("/v1/batch/rb_1", {
      headers: { authorization: `Bearer ${rig.token}` },
    });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ status: "in_progress", completed: 300, total: 812 });
    expect(rig.gateway.getCalls).toEqual(["rb_1"]);
  });

  it("streams ended results as relay-shaped JSON keyed by custom_id (§4.3)", async () => {
    const rig = await makeRig();
    rig.gateway.status = {
      id: "rb_1",
      processing_status: "ended",
      request_counts: { processing: 0, succeeded: 2, errored: 1, canceled: 0, expired: 0 },
    };
    rig.gateway.resultLines = [
      JSON.stringify({
        custom_id: "b",
        result: { type: "succeeded", message: { content: [{ type: "text", text: "B-label" }] } },
      }),
      JSON.stringify({
        custom_id: "a",
        result: {
          type: "succeeded",
          // Multi-block content concatenates, mirroring the device-side parser.
          message: { content: [{ type: "text", text: "A-" }, { type: "text", text: "label" }] },
        },
      }),
      JSON.stringify({ custom_id: "c", result: { type: "errored", error: { type: "invalid_request" } } }),
    ];
    const res = await rig.app.request("/v1/batch/rb_1", {
      headers: { authorization: `Bearer ${rig.token}` },
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as { status: string; results: Array<Record<string, string>> };
    expect(body.status).toBe("ended");
    const byId = new Map(body.results.map((r) => [r.custom_id, r]));
    expect(byId.get("a")?.text).toBe("A-label");
    expect(byId.get("b")?.text).toBe("B-label");
    expect(byId.get("c")?.error).toBe("errored");
    expect(rig.gateway.resultsCalls).toEqual(["rb_1"]);
  });
});
