/**
 * The batch relay app (docs/batch-relay-design.md §4) — a Hono app with injected seams so every
 * route is testable offline.
 *
 * Invariants carried by construction:
 * - **No chunk content persists here** (§3.2 / NFR-PRV-04): the redacting logger is registered
 *   before any route; the UsageStore interface has no content field; results stream through a
 *   line-at-a-time transform, never a buffered body on disk.
 * - **The device never picks the model** (§4.4): the request carries `model_class`; the mapping
 *   lives in `models.ts`. A `model` key in the request body is simply never read.
 * - **Errors follow §4.5**: 401 invalid/expired token, 402 plan without the lane, 429 daily cap,
 *   502 upstream failure.
 */
import { Hono } from "hono";
import type { KeyObject } from "node:crypto";

import { AuthError, verifyLicense } from "./auth.js";
import type { AnthropicGateway } from "./gateway.js";
import { UpstreamError } from "./gateway.js";
import { redactingLogger, type LogFn, type LogVars } from "./logging.js";
import { MAX_TOKENS_BY_CLASS, MODEL_BY_CLASS } from "./models.js";
import { parseRelayBatchRequest, type AnthropicBatchRequestItem, type RelayResult } from "./types.js";
import { utcDate, type UsageStore } from "./usage.js";

export interface RelayDeps {
  gateway: AnthropicGateway;
  usage: UsageStore;
  /** The licence API's Ed25519 public key (§4.1 — the SAME key that signs the `v1.` licence
   * tokens; see `licensePublicKeyFromB64`). */
  licensePublicKey: KeyObject;
  /** Per-license daily chunk cap (OPEN-B2: the concrete number comes from the cost model; the
   * enforcement lives here either way). */
  dailyChunkCap: number;
  log: LogFn;
  /** Injected clock for tests. */
  now?: () => Date;
}

/** Map one Anthropic JSONL results line to the relay's result shape (§4.3). Content passes
 * through; nothing is retained. Malformed lines surface as error entries, never silent drops. */
export function transformResultLine(line: string): RelayResult {
  let v: unknown;
  try {
    v = JSON.parse(line);
  } catch {
    return { custom_id: "", error: "malformed result line" };
  }
  const o = (typeof v === "object" && v !== null ? v : {}) as Record<string, unknown>;
  const customId = typeof o.custom_id === "string" ? o.custom_id : "";
  const result = (typeof o.result === "object" && o.result !== null ? o.result : {}) as Record<string, unknown>;
  const type = typeof result.type === "string" ? result.type : "missing result.type";
  if (type !== "succeeded") return { custom_id: customId, error: type };
  const message = (typeof result.message === "object" && result.message !== null ? result.message : {}) as Record<string, unknown>;
  const content = Array.isArray(message.content) ? message.content : [];
  const text = content
    .map((b: unknown) => {
      const block = (typeof b === "object" && b !== null ? b : {}) as Record<string, unknown>;
      return block.type === "text" && typeof block.text === "string" ? block.text : "";
    })
    .join("");
  return { custom_id: customId, text };
}

export function createApp(deps: RelayDeps): Hono<{ Variables: LogVars }> {
  const app = new Hono<{ Variables: LogVars }>();
  const now = deps.now ?? (() => new Date());

  // FIRST middleware: the only logger. Registered before any route exists so no handler can be
  // reached without it, and it cannot see bodies at all (logging.ts).
  app.use("*", redactingLogger(deps.log));

  app.onError((err, c) => {
    if (err instanceof AuthError) {
      return c.json({ error: err.message }, err.status);
    }
    if (err instanceof UpstreamError) {
      // Status only — upstream bodies can quote request content and never reach a log or client.
      return c.json({ error: "upstream failure" }, 502);
    }
    // Unknown failure: a fixed string. err.message could contain interpolated request data.
    return c.json({ error: "internal error" }, 500);
  });

  const nowSecs = (): number => Math.floor(now().getTime() / 1000);

  app.post("/v1/batch", async (c) => {
    const license = verifyLicense(c.req.header("authorization"), deps.licensePublicKey, nowSecs());
    c.set("relayLogLicense", license.licenseId);

    let raw: unknown;
    try {
      raw = await c.req.json();
    } catch {
      return c.json({ error: "body must be JSON" }, 400);
    }
    const parsed = parseRelayBatchRequest(raw);
    if (typeof parsed === "string") {
      return c.json({ error: parsed }, 400);
    }

    // Daily cap (§2.2 — the reason the relay exists: the limit is enforced where the key lives).
    const date = utcDate(now());
    const used = await deps.usage.usedOn(license.licenseId, date);
    if (used + parsed.items.length > deps.dailyChunkCap) {
      return c.json({ error: "daily chunk cap reached" }, 429);
    }

    // Forward with the server-chosen model (§4.4) and the custom_ids untouched.
    const requests: AnthropicBatchRequestItem[] = parsed.items.map((it) => ({
      custom_id: it.custom_id,
      params: {
        model: MODEL_BY_CLASS[parsed.model_class],
        max_tokens: MAX_TOKENS_BY_CLASS[parsed.model_class],
        messages: [{ role: "user", content: it.chunk }],
      },
    }));
    const created = await deps.gateway.createBatch(requests);

    await deps.usage.record(license.licenseId, date, parsed.items.length, created.id);
    c.set("relayLogChunks", parsed.items.length);
    return c.json({ batch_id: created.id, accepted: parsed.items.length }, 202);
  });

  app.get("/v1/batch/:id", async (c) => {
    const license = verifyLicense(c.req.header("authorization"), deps.licensePublicKey, nowSecs());
    c.set("relayLogLicense", license.licenseId);
    const id = c.req.param("id");

    // Ownership check (§4.1): a licence may only read back batches IT created. Anthropic batch
    // ids are guessable, and without this any licensee could stream any other user's Dream-Cycle
    // results. "Unknown" and "someone else's" answer identically — 404, no oracle.
    const owner = await deps.usage.batchOwner(id);
    if (owner !== license.licenseId) {
      return c.json({ error: "not found" }, 404);
    }

    const status = await deps.gateway.getBatch(id);
    if (status.processing_status !== "ended") {
      const counts = status.request_counts;
      const total =
        counts.processing + counts.succeeded + counts.errored + counts.canceled + counts.expired;
      return c.json({
        status: status.processing_status,
        completed: counts.succeeded + counts.errored,
        total,
      });
    }

    // Ended: stream the results through, one transformed line at a time (§4.3). Pull-based so
    // the relay holds at most one result line in memory — the response is built as the client
    // reads it, and nothing is retained afterwards (§3.2).
    const lines = deps.gateway.streamResults(id)[Symbol.asyncIterator]();
    const encoder = new TextEncoder();
    let first = true;
    let opened = false;
    const body = new ReadableStream<Uint8Array>({
      async pull(controller) {
        if (!opened) {
          opened = true;
          controller.enqueue(encoder.encode('{"status":"ended","results":['));
          return;
        }
        const next = await lines.next();
        if (next.done) {
          controller.enqueue(encoder.encode("]}"));
          controller.close();
          return;
        }
        const item = transformResultLine(next.value);
        controller.enqueue(encoder.encode((first ? "" : ",") + JSON.stringify(item)));
        first = false;
      },
      async cancel() {
        await lines.return?.();
      },
    });
    return new Response(body, {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  });

  return app;
}
