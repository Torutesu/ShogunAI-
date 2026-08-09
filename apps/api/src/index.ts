/**
 * Boot the relay (docs/batch-relay-design.md).
 *
 * Env:
 * - ANTHROPIC_API_KEY        — the operator key. Lives ONLY here (server secret); never logged,
 *                              never in a response, never in the desktop binary (§2.1).
 * - LICENSE_JWT_PUBLIC_KEY   — SPKI PEM of the license API's ES256 public key (§4.1).
 * - DAILY_CHUNK_CAP          — per-license daily chunk cap (default 2000; OPEN-B2).
 * - USAGE_FILE               — usage ledger path (default ./data/usage.json). Aggregates only.
 * - PORT                     — listen port (default 8787).
 */
import { serve } from "@hono/node-server";
import { importSPKI } from "jose";

import { createApp } from "./app.js";
import { FetchAnthropicGateway } from "./gateway.js";
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
const publicKeyPem = requireEnv("LICENSE_JWT_PUBLIC_KEY");
const dailyChunkCap = Number(process.env.DAILY_CHUNK_CAP ?? "2000");
const usageFile = process.env.USAGE_FILE ?? "./data/usage.json";
const port = Number(process.env.PORT ?? "8787");

const licensePublicKey = await importSPKI(publicKeyPem, "ES256");

const app = createApp({
  gateway: new FetchAnthropicGateway(apiKey),
  usage: new JsonFileUsageStore(usageFile),
  licensePublicKey,
  dailyChunkCap,
  // The console line carries method/path/status/counters only (logging.ts) — never a body.
  log: (line) => console.log(line),
});

serve({ fetch: app.fetch, port }, (info) => {
  console.log(`batch relay listening on :${info.port}`);
});
