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

/** Parse + validate an unknown JSON body into a RelayBatchRequest, or return an error string. */
export function parseRelayBatchRequest(body: unknown): RelayBatchRequest | string {
  if (typeof body !== "object" || body === null) return "body must be a JSON object";
  const b = body as Record<string, unknown>;
  if (typeof b.purpose !== "string" || b.purpose.length === 0) return "purpose must be a non-empty string";
  if (!isModelClass(b.model_class)) return "model_class must be one of classify|summarize|brief";
  if (!Array.isArray(b.items) || b.items.length === 0) return "items must be a non-empty array";
  const items: RelayBatchItem[] = [];
  for (const raw of b.items) {
    if (typeof raw !== "object" || raw === null) return "each item must be an object";
    const it = raw as Record<string, unknown>;
    if (typeof it.custom_id !== "string" || it.custom_id.length === 0) return "each item needs a custom_id";
    if (typeof it.chunk !== "string" || it.chunk.length === 0) return "each item needs a chunk";
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
