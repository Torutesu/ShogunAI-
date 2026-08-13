/**
 * Per-licence request rate limiting (docs/batch-relay-design.md §2.2, audit P1).
 *
 * The daily chunk cap bounds how much a licence can spend in a day; this bounds how fast. They
 * are different guards: 50 parallel submissions can respect the cap and still knock the relay
 * (and the Anthropic account's own rate limits) over, and a leaked token's first move is a loop.
 *
 * In-memory on purpose, and only correct for the single-process v1 relay — the same scope note
 * as `JsonFileUsageStore` (OPEN-B1: a second instance needs a shared store for both). The clock
 * is a parameter, so the whole thing is deterministic under test.
 */

export interface RateLimiter {
  /** Consume one slot for `key` at `nowMs`. `false` means the caller must back off. */
  take(key: string, nowMs: number): boolean;
}

/** Never limits. The default when a deployment has its own edge rate limiting in front. */
export class NoRateLimit implements RateLimiter {
  take(): boolean {
    return true;
  }
}

interface Bucket {
  /** Tokens left, fractional between refills. */
  tokens: number;
  /** When `tokens` was last brought up to date. Doubles as the idle-sweep timestamp. */
  at: number;
}

/** Buckets idle for this long are dropped on the next sweep — the map must not grow with every
 * licence that ever called. */
const IDLE_EVICT_MS = 10 * 60_000;
/** Sweep at most this often, and only when the map is worth sweeping. */
const SWEEP_EVERY_MS = 60_000;
const SWEEP_MIN_SIZE = 256;

export class TokenBucketLimiter implements RateLimiter {
  private readonly buckets = new Map<string, Bucket>();
  private lastSweep = 0;

  /**
   * @param burst      how many requests may arrive back-to-back (bucket size)
   * @param perMinute  sustained refill rate
   */
  constructor(
    private readonly burst: number,
    private readonly perMinute: number,
  ) {}

  take(key: string, nowMs: number): boolean {
    this.sweep(nowMs);
    const b = this.buckets.get(key);
    if (!b) {
      this.buckets.set(key, { tokens: this.burst - 1, at: nowMs });
      return true;
    }
    const refill = ((nowMs - b.at) / 60_000) * this.perMinute;
    b.tokens = Math.min(this.burst, b.tokens + Math.max(0, refill));
    b.at = nowMs;
    if (b.tokens < 1) return false;
    b.tokens -= 1;
    return true;
  }

  private sweep(nowMs: number): void {
    if (this.buckets.size < SWEEP_MIN_SIZE || nowMs - this.lastSweep < SWEEP_EVERY_MS) return;
    this.lastSweep = nowMs;
    for (const [key, b] of this.buckets) {
      if (nowMs - b.at > IDLE_EVICT_MS) this.buckets.delete(key);
    }
  }
}
