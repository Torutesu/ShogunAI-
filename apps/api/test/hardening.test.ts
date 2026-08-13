/** Audit P1 (plan 1-2): the guards that keep one leaked licence from burning the operator's
 * Anthropic key — an atomic cap under concurrency, bounded input, a metering failure that
 * refuses rather than spends, and a per-licence rate limit. */
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { MAX_BODY_BYTES } from "../src/app.js";
import { TokenBucketLimiter, type RateLimiter } from "../src/ratelimit.js";
import { MAX_CHUNK_BYTES, MAX_ITEMS } from "../src/types.js";
import { JsonFileUsageStore } from "../src/usage.js";
import { makeRig, postBatch, sampleBody } from "./helpers.js";

function bodyWithChunks(n: number): unknown {
  return {
    purpose: "consolidation",
    model_class: "classify",
    items: Array.from({ length: n }, (_, i) => ({ custom_id: String(i), chunk: `c${i}` })),
  };
}

async function tempLedger(contents?: string): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "shogun-relay-"));
  const path = join(dir, "usage.json");
  if (contents !== undefined) await writeFile(path, contents, "utf8");
  return path;
}

describe("daily cap under concurrency (§2.2)", () => {
  it("never lets simultaneous submissions cross the cap, even by one chunk", async () => {
    // The file store is the one with awaits between "read used" and "write used" — the shape a
    // read-then-write cap gets wrong. 50 submissions of 1 chunk against a cap of 10.
    const usage = new JsonFileUsageStore(await tempLedger());
    const rig = await makeRig({ usage, dailyChunkCap: 10 });

    const results = await Promise.all(
      Array.from({ length: 50 }, () => postBatch(rig, bodyWithChunks(1))),
    );
    const accepted = results.filter((r) => r.status === 202).length;
    expect(accepted).toBe(10);
    expect(results.filter((r) => r.status === 429)).toHaveLength(40);
    // And the ledger agrees with what was actually forwarded.
    expect(rig.gateway.createCalls).toHaveLength(10);
    expect(await usage.usedOn("lic_test_1", new Date().toISOString().slice(0, 10))).toBe(10);
  });

  it("releases the reservation when the upstream call fails, so the day is not silently spent", async () => {
    const usage = new JsonFileUsageStore(await tempLedger());
    const rig = await makeRig({ usage, dailyChunkCap: 10 });
    const date = new Date().toISOString().slice(0, 10);
    const realCreate = rig.gateway.createBatch.bind(rig.gateway);

    rig.gateway.createBatch = () => Promise.reject(new Error("upstream down"));
    const failed = await postBatch(rig, bodyWithChunks(6));
    expect(failed.status).toBe(500);
    expect(await usage.usedOn("lic_test_1", date)).toBe(0);

    // The whole cap is still available once upstream recovers — a failed submission must not
    // eat six chunks of the user's day.
    rig.gateway.createBatch = realCreate;
    expect((await postBatch(rig, bodyWithChunks(10))).status).toBe(202);
    expect(await usage.usedOn("lic_test_1", date)).toBe(10);
  });
});

describe("bounded input", () => {
  it("refuses more than MAX_ITEMS entries before touching the gateway", async () => {
    const rig = await makeRig({ dailyChunkCap: 10_000_000 });
    const res = await postBatch(rig, bodyWithChunks(MAX_ITEMS + 1));
    expect(res.status).toBe(400);
    expect(rig.gateway.createCalls).toHaveLength(0);
  });

  it("refuses an oversized chunk", async () => {
    const rig = await makeRig();
    const res = await postBatch(rig, {
      purpose: "consolidation",
      model_class: "classify",
      items: [{ custom_id: "1", chunk: "x".repeat(MAX_CHUNK_BYTES + 1) }],
    });
    expect(res.status).toBe(400);
    expect(rig.gateway.createCalls).toHaveLength(0);
  });

  it("refuses a body past the hard ceiling with 413", async () => {
    const rig = await makeRig();
    const huge = JSON.stringify({
      purpose: "consolidation",
      model_class: "classify",
      items: [{ custom_id: "1", chunk: "x".repeat(MAX_BODY_BYTES + 1024) }],
    });
    const res = await rig.app.request("/v1/batch", {
      method: "POST",
      headers: {
        authorization: `Bearer ${rig.token}`,
        "content-type": "application/json",
        "content-length": String(Buffer.byteLength(huge)),
      },
      body: huge,
    });
    expect(res.status).toBe(413);
    expect(rig.gateway.createCalls).toHaveLength(0);
  });
});

describe("metering failure fails closed (§4.5)", () => {
  it("answers 503 — not 429, not success — when the ledger cannot be read", async () => {
    const usage = new JsonFileUsageStore(await tempLedger("{ this is not json"));
    const rig = await makeRig({ usage });
    const res = await postBatch(rig, sampleBody);
    expect(res.status).toBe(503);
    expect(rig.gateway.createCalls).toHaveLength(0);
  });

  it("treats an absent ledger as a fresh day, not a failure", async () => {
    const path = await tempLedger(); // never written
    const rig = await makeRig({ usage: new JsonFileUsageStore(path) });
    const res = await postBatch(rig, sampleBody);
    expect(res.status).toBe(202);
    // …and the first accepted submission creates it.
    const written: unknown = JSON.parse(await readFile(path, "utf8"));
    expect(written).toHaveProperty("days");
  });
});

describe("per-licence rate limit", () => {
  it("refuses a burst past the bucket and lets a different licence through", async () => {
    let nowMs = 1_700_000_000_000;
    const rig = await makeRig({
      dailyChunkCap: 10_000,
      rateLimit: new TokenBucketLimiter(3, 3),
      now: () => new Date(nowMs),
    });

    expect((await postBatch(rig, bodyWithChunks(1))).status).toBe(202);
    expect((await postBatch(rig, bodyWithChunks(1))).status).toBe(202);
    expect((await postBatch(rig, bodyWithChunks(1))).status).toBe(202);
    const over = await postBatch(rig, bodyWithChunks(1));
    expect(over.status).toBe(429);
    expect(await over.json()).toEqual({ error: "too many requests" });
    expect(rig.gateway.createCalls).toHaveLength(3);

    // A minute later the bucket has refilled.
    nowMs += 60_000;
    expect((await postBatch(rig, bodyWithChunks(1))).status).toBe(202);
  });

  it("meters the bucket per licence, not globally", async () => {
    const { signLicense } = await import("./helpers.js");
    let nowMs = 1_700_000_000_000;
    const rig = await makeRig({
      dailyChunkCap: 10_000,
      rateLimit: new TokenBucketLimiter(1, 1),
      now: () => new Date(nowMs),
    });
    const other = await signLicense(rig.keys.privateKey, { sub: "lic_other" });

    expect((await postBatch(rig, bodyWithChunks(1))).status).toBe(202);
    expect((await postBatch(rig, bodyWithChunks(1))).status).toBe(429);
    expect((await postBatch(rig, bodyWithChunks(1), other)).status).toBe(202);
  });

  it("the limiter itself refills at the configured rate", () => {
    const limiter: RateLimiter = new TokenBucketLimiter(2, 60); // 1/sec sustained
    expect(limiter.take("a", 0)).toBe(true);
    expect(limiter.take("a", 0)).toBe(true);
    expect(limiter.take("a", 0)).toBe(false);
    expect(limiter.take("a", 999)).toBe(false); // not quite a second
    expect(limiter.take("a", 1000)).toBe(true);
  });
});
