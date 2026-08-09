/**
 * The Anthropic Batch API seam (docs/batch-relay-design.md §1 step ④).
 *
 * The app is written against this interface so every route is testable with an in-memory fake
 * and no socket. The real implementation holds the ONLY Anthropic key in the whole product
 * (server secret; the desktop binary never sees one) and passes chunk content straight through —
 * results are exposed as a line stream precisely so no caller is tempted to buffer a whole
 * results body into memory or, worse, onto disk (§3.2).
 */
import type { AnthropicBatchRequestItem } from "./types.js";

export interface BatchCounts {
  processing: number;
  succeeded: number;
  errored: number;
  canceled: number;
  expired: number;
}

export interface BatchStatus {
  id: string;
  processing_status: "in_progress" | "canceling" | "ended" | string;
  request_counts: BatchCounts;
}

/** Upstream (Anthropic-side) failure. Mapped to 502 by the app — never echoed with body detail,
 * because upstream error bodies can quote request content. */
export class UpstreamError extends Error {
  constructor(
    public readonly step: string,
    public readonly status: number,
  ) {
    // Status and step only — deliberately no upstream body text.
    super(`anthropic ${step} failed with HTTP ${status}`);
    this.name = "UpstreamError";
  }
}

export interface AnthropicGateway {
  /** `POST /v1/messages/batches`. */
  createBatch(requests: AnthropicBatchRequestItem[]): Promise<BatchStatus>;
  /** `GET /v1/messages/batches/{id}`. */
  getBatch(id: string): Promise<BatchStatus>;
  /** `GET /v1/messages/batches/{id}/results`, yielded line by line (JSONL). Holds at most one
   * partial line in memory at a time; never writes anywhere. */
  streamResults(id: string): AsyncIterable<string>;
}

const ANTHROPIC_VERSION = "2023-06-01";

function parseCounts(v: unknown): BatchCounts {
  const c = (typeof v === "object" && v !== null ? v : {}) as Record<string, unknown>;
  const n = (k: string): number => (typeof c[k] === "number" ? (c[k] as number) : 0);
  return {
    processing: n("processing"),
    succeeded: n("succeeded"),
    errored: n("errored"),
    canceled: n("canceled"),
    expired: n("expired"),
  };
}

function parseStatus(step: string, status: number, body: unknown): BatchStatus {
  const b = (typeof body === "object" && body !== null ? body : {}) as Record<string, unknown>;
  if (typeof b.id !== "string" || typeof b.processing_status !== "string") {
    throw new UpstreamError(`${step} (malformed response)`, status);
  }
  return {
    id: b.id,
    processing_status: b.processing_status,
    request_counts: parseCounts(b.request_counts),
  };
}

/** The real, fetch-based gateway. Constructed once at boot with the server-side key. */
export class FetchAnthropicGateway implements AnthropicGateway {
  constructor(
    private readonly apiKey: string,
    private readonly baseUrl: string = "https://api.anthropic.com",
  ) {}

  private headers(): Record<string, string> {
    return {
      "x-api-key": this.apiKey,
      "anthropic-version": ANTHROPIC_VERSION,
      "content-type": "application/json",
    };
  }

  async createBatch(requests: AnthropicBatchRequestItem[]): Promise<BatchStatus> {
    const res = await fetch(`${this.baseUrl}/v1/messages/batches`, {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify({ requests }),
    });
    if (!res.ok) throw new UpstreamError("create", res.status);
    return parseStatus("create", res.status, await res.json());
  }

  async getBatch(id: string): Promise<BatchStatus> {
    const res = await fetch(`${this.baseUrl}/v1/messages/batches/${encodeURIComponent(id)}`, {
      headers: this.headers(),
    });
    if (!res.ok) throw new UpstreamError("poll", res.status);
    return parseStatus("poll", res.status, await res.json());
  }

  async *streamResults(id: string): AsyncIterable<string> {
    const res = await fetch(
      `${this.baseUrl}/v1/messages/batches/${encodeURIComponent(id)}/results`,
      { headers: this.headers() },
    );
    if (!res.ok || res.body === null) throw new UpstreamError("results", res.status);
    const decoder = new TextDecoder();
    const reader = res.body.getReader();
    let carry = "";
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        carry += decoder.decode(value, { stream: true });
        let nl: number;
        while ((nl = carry.indexOf("\n")) >= 0) {
          const line = carry.slice(0, nl);
          carry = carry.slice(nl + 1);
          if (line.trim().length > 0) yield line;
        }
      }
      carry += decoder.decode();
      if (carry.trim().length > 0) yield carry;
    } finally {
      reader.releaseLock();
    }
  }
}
