/** Shared test rig: ES256 key pair, license signing, the in-memory gateway fake, and a
 * fully-wired app whose logger writes into an inspectable array. */
import { generateKeyPair, SignJWT, type KeyLike } from "jose";

import { createApp, type RelayDeps } from "../src/app.js";
import type { AnthropicGateway, BatchStatus } from "../src/gateway.js";
import type { AnthropicBatchRequestItem } from "../src/types.js";
import { InMemoryUsageStore } from "../src/usage.js";

export interface Keys {
  privateKey: KeyLike;
  publicKey: KeyLike;
}

export async function makeKeys(): Promise<Keys> {
  const { privateKey, publicKey } = await generateKeyPair("ES256");
  return { privateKey, publicKey };
}

export async function signLicense(
  privateKey: KeyLike,
  opts: { sub?: string; plan?: string; expiresIn?: string } = {},
): Promise<string> {
  const jwt = new SignJWT({ plan: opts.plan ?? "standard" })
    .setProtectedHeader({ alg: "ES256" })
    .setSubject(opts.sub ?? "lic_test_1")
    .setIssuedAt()
    .setExpirationTime(opts.expiresIn ?? "24h");
  return jwt.sign(privateKey);
}

/**
 * The gateway fake. Deliberately persistence-free: it keeps the forwarded requests reachable for
 * assertions (that is the test observing the passthrough, not the relay storing content) and
 * replays canned status/results. `streamResults` yields from the canned lines — the app must
 * consume them incrementally.
 */
export class FakeGateway implements AnthropicGateway {
  createCalls: AnthropicBatchRequestItem[][] = [];
  getCalls: string[] = [];
  resultsCalls: string[] = [];

  nextBatchId = "rb_fake_1";
  status: BatchStatus = {
    id: "rb_fake_1",
    processing_status: "in_progress",
    request_counts: { processing: 1, succeeded: 0, errored: 0, canceled: 0, expired: 0 },
  };
  resultLines: string[] = [];

  createBatch(requests: AnthropicBatchRequestItem[]): Promise<BatchStatus> {
    this.createCalls.push(requests);
    return Promise.resolve({
      id: this.nextBatchId,
      processing_status: "in_progress",
      request_counts: {
        processing: requests.length,
        succeeded: 0,
        errored: 0,
        canceled: 0,
        expired: 0,
      },
    });
  }

  getBatch(id: string): Promise<BatchStatus> {
    this.getCalls.push(id);
    return Promise.resolve(this.status);
  }

  async *streamResults(id: string): AsyncIterable<string> {
    this.resultsCalls.push(id);
    for (const line of this.resultLines) {
      yield line;
    }
  }
}

export interface Rig {
  app: ReturnType<typeof createApp>;
  gateway: FakeGateway;
  usage: InMemoryUsageStore;
  logLines: string[];
  keys: Keys;
  token: string;
}

export async function makeRig(overrides: Partial<Omit<RelayDeps, "log">> = {}): Promise<Rig> {
  const keys = await makeKeys();
  const gateway = new FakeGateway();
  const usage = new InMemoryUsageStore();
  const logLines: string[] = [];
  const app = createApp({
    gateway,
    usage,
    licensePublicKey: keys.publicKey,
    dailyChunkCap: 100,
    log: (line) => logLines.push(line),
    ...overrides,
  });
  const token = await signLicense(keys.privateKey);
  return { app, gateway, usage, logLines, keys, token };
}

export async function postBatch(
  rig: Rig,
  body: unknown,
  token: string = rig.token,
): Promise<Response> {
  return rig.app.request("/v1/batch", {
    method: "POST",
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

export const sampleBody = {
  purpose: "consolidation",
  model_class: "classify",
  items: [
    { custom_id: "1234", chunk: "TOP-SECRET-CHUNK-ALPHA" },
    { custom_id: "5678", chunk: "TOP-SECRET-CHUNK-BRAVO" },
  ],
};
