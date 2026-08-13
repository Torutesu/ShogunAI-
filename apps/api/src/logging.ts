/**
 * Body-redacting request logging (docs/batch-relay-design.md §3.2 / NFR-PRV-04).
 *
 * Written FIRST, before any route handler: the relay's core invariant is that chunk content is
 * never persisted, and logs are the easiest place to break that by accident. This middleware is
 * therefore the only sanctioned way a request leaves a trace, and it is structurally unable to
 * leak: it reads nothing but method, path, status, duration, and (on POST) the *count* of
 * accepted items a handler chose to expose via `relay-log-*` context vars. No body, no headers
 * (the Authorization header is a bearer credential), no query strings beyond the path.
 */
import { createHash } from "node:crypto";
import type { MiddlewareHandler } from "hono";

export type LogFn = (line: string) => void;

/** Stable short hash of a licence id, for the log line.
 *
 * The id is not user content, but `license=<sub>` still turns the console into a per-customer
 * activity timeline, and the log sink is usually less protected than the usage ledger. A hash
 * keeps what the line is actually for — correlating requests within an incident — without the
 * identifier itself. */
export function logId(licenseId: string): string {
  return createHash("sha256").update(licenseId).digest("hex").slice(0, 12);
}

/** Context vars a handler may set for the log line. Numbers only — never content. */
export interface LogVars {
  /** Number of chunks accepted (set by POST /v1/batch). */
  relayLogChunks?: number;
  /** License id (an opaque token id, not user content). */
  relayLogLicense?: string;
}

export function redactingLogger(log: LogFn): MiddlewareHandler<{ Variables: LogVars }> {
  return async (c, next) => {
    const start = Date.now();
    try {
      await next();
    } finally {
      const ms = Date.now() - start;
      const chunks = c.get("relayLogChunks");
      const license = c.get("relayLogLicense");
      const extras = [
        typeof chunks === "number" ? `chunks=${chunks}` : null,
        typeof license === "string" ? `license=${logId(license)}` : null,
      ]
        .filter((v): v is string => v !== null)
        .join(" ");
      // Path only — never the body, never a header value.
      log(`${c.req.method} ${new URL(c.req.url).pathname} ${c.res.status} ${ms}ms${extras ? " " + extras : ""}`);
    }
  };
}
