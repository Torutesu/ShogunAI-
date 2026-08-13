/**
 * Wire types for the batch relay (docs/batch-relay-design.md §4).
 *
 * The device sends a `model_class` *intent*, never a model id (§4.4): a client that can pick the
 * model can pick an expensive one, so the mapping to a concrete model lives server-side
 * (`models.ts`). Chunk content passes through this process and is never written to disk or logs
 * (§3.2 / NFR-PRV-04).
 */

/** The intents a device may send. The relay maps each to a concrete model. */
export const MODEL_CLASSES = ["classify", "summarize", "brief"] as const;
export type ModelClass = (typeof MODEL_CLASSES)[number];

export function isModelClass(v: unknown): v is ModelClass {
  return typeof v === "string" && (MODEL_CLASSES as readonly string[]).includes(v);
}

/** One item of `POST /v1/batch` (§4.2). */
export interface RelayBatchItem {
  custom_id: string;
  chunk: string;
}

/** The `POST /v1/batch` request body (§4.2). */
export interface RelayBatchRequest {
  purpose: string;
  model_class: ModelClass;
  items: RelayBatchItem[];
}

/** Ceiling on items per submission. The nightly Dream Cycle submits its whole night in one
 * call, so this is generous — it exists to stop one request from becoming an unbounded fan-out
 * against the operator's key, not to shape normal traffic. */
export const MAX_ITEMS = 1000;

/** Ceiling on one chunk, in UTF-8 bytes. The largest legitimate chunk is a whole meeting
 * transcript folded into a single recap prompt; 256 KB covers a very long meeting. */
export const MAX_CHUNK_BYTES = 256 * 1024;

/** Ceiling on a `custom_id`. It is an opaque key the device chooses (an event id, a session
 * id) and is echoed back verbatim. */
export const MAX_CUSTOM_ID_BYTES = 256;

/** Parse + validate an unknown JSON body into a RelayBatchRequest, or return an error string. */
export function parseRelayBatchRequest(body: unknown): RelayBatchRequest | string {
  if (typeof body !== "object" || body === null) return "body must be a JSON object";
  const b = body as Record<string, unknown>;
  if (typeof b.purpose !== "string" || b.purpose.length === 0) return "purpose must be a non-empty string";
  if (b.purpose.length > MAX_CUSTOM_ID_BYTES) return "purpose is too long";
  if (!isModelClass(b.model_class)) return "model_class must be one of classify|summarize|brief";
  if (!Array.isArray(b.items) || b.items.length === 0) return "items must be a non-empty array";
  if (b.items.length > MAX_ITEMS) return `items must hold at most ${MAX_ITEMS} entries`;
  const items: RelayBatchItem[] = [];
  for (const raw of b.items) {
    if (typeof raw !== "object" || raw === null) return "each item must be an object";
    const it = raw as Record<string, unknown>;
    if (typeof it.custom_id !== "string" || it.custom_id.length === 0) return "each item needs a custom_id";
    if (Buffer.byteLength(it.custom_id, "utf8") > MAX_CUSTOM_ID_BYTES) return "a custom_id is too long";
    if (typeof it.chunk !== "string" || it.chunk.length === 0) return "each item needs a chunk";
    if (Buffer.byteLength(it.chunk, "utf8") > MAX_CHUNK_BYTES) return "a chunk is too large";
    items.push({ custom_id: it.custom_id, chunk: it.chunk });
  }
  return { purpose: b.purpose, model_class: b.model_class, items };
}

/** One request line forwarded to Anthropic `POST /v1/messages/batches`. */
export interface AnthropicBatchRequestItem {
  custom_id: string;
  params: {
    model: string;
    max_tokens: number;
    messages: Array<{ role: "user"; content: string }>;
  };
}

/** A relay result entry (§4.3): text on success, error type otherwise. */
export type RelayResult =
  | { custom_id: string; text: string }
  | { custom_id: string; error: string };
