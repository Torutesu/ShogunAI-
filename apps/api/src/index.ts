/**
 * Boot the relay (docs/batch-relay-design.md).
 *
 * Env:
 * - ANTHROPIC_API_KEY        — the operator key. Lives ONLY here (server secret); never logged,
 *                              never in a response, never in the desktop binary (§2.1).
 * - LICENSE_PUBKEY_B64       — base64 of the licence API's raw 32-byte Ed25519 public key
 *                              (§4.1; the `SHOGUN_LICENSE_PUBKEY` value that
 *                              `scripts/gen-license-keypair.mjs` prints).
 * - DAILY_CHUNK_CAP          — per-license daily chunk cap (default 2000; OPEN-B2).
 * - RATE_BURST               — per-licence back-to-back request allowance (default 12).
 * - RATE_PER_MIN             — per-licence sustained requests/minute (default 12).
 * - USAGE_FILE               — usage ledger path (default ./data/usage.json). Aggregates only.
 * - PORT                     — listen port (default 8787).
 */
import { serve } from "@hono/node-server";

import { licensePublicKeyFromB64 } from "./auth.js";
import { createApp } from "./app.js";
import { FetchAnthropicGateway } from "./gateway.js";
import { TokenBucketLimiter } from "./ratelimit.js";
import { JsonFileUsageStore } from "./usage.js";

function requireEnv(name: string): string {
  const v = process.env[name];
  if (!v) {
    // Name only — never a value.
    console.error(`missing required env: ${name}`);
    process.exit(1);
  }
  return v;
}

const apiKey = requireEnv("ANTHROPIC_API_KEY");
const publicKeyB64 = requireEnv("LICENSE_PUBKEY_B64");
const dailyChunkCap = Number(process.env.DAILY_CHUNK_CAP ?? "2000");
const usageFile = process.env.USAGE_FILE ?? "./data/usage.json";
const rateBurst = Number(process.env.RATE_BURST ?? "12");
const ratePerMin = Number(process.env.RATE_PER_MIN ?? "12");
const port = Number(process.env.PORT ?? "8787");

const licensePublicKey = licensePublicKeyFromB64(publicKeyB64);

const app = createApp({
  gateway: new FetchAnthropicGateway(apiKey),
  usage: new JsonFileUsageStore(usageFile),
  licensePublicKey,
  dailyChunkCap,
  rateLimit: new TokenBucketLimiter(rateBurst, ratePerMin),
  // The console line carries method/path/status/counters only (logging.ts) — never a body.
  log: (line) => console.log(line),
});

serve({ fetch: app.fetch, port }, (info) => {
  console.log(`batch relay listening on :${info.port}`);
});
