/**
 * Usage metering (docs/batch-relay-design.md §1 step ③ / §3.2).
 *
 * What may persist server-side is EXACTLY the aggregate needed for billing and cap enforcement:
 * license id, UTC date, chunk count, batch id. The interface has no field that could carry chunk
 * content, so a store cannot leak it even by mistake — the same "no text column" trick the
 * device's traceability_log uses.
 *
 * The cap is enforced as **reserve-then-commit**, not read-then-write: the check and the
 * increment are one atomic step inside the store, because the whole point of the relay is that
 * the spend limit holds when N submissions arrive at once (§2.2). A reservation that never
 * becomes a batch (upstream refused) is released.
 *
 * Store choice (v1): a JSON file, not better-sqlite3. Justification: the data is a tiny
 * two-level counter map ({date → {license → chunks}}, thousands of entries at most, pruned after
 * a few days), the v1 relay is a single process, and a JSON file needs no native build toolchain
 * on the deploy host. Writes are atomic (temp file + rename) so a crash never corrupts the
 * ledger. The `UsageStore` seam is the migration path to a real DB when the relay grows a second
 * instance (OPEN-B1/B2).
 */
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";

/** The outcome of a reservation. `unavailable` is deliberately distinct from `capped`: a
 * ledger we cannot read is not "you spent your quota", and answering 429 for it would hide an
 * operational failure behind a normal-looking client error. */
export type Reservation = "ok" | "capped" | "unavailable";

export interface UsageStore {
  /** Chunks already accepted for `licenseId` on `date` (UTC `YYYY-MM-DD`). */
  usedOn(licenseId: string, date: string): Promise<number>;
  /** Atomically reserve `chunks` against `cap` for `licenseId` on `date`. Reserves nothing
   * unless the whole amount fits — a partially accepted batch is not a thing the device can
   * act on. */
  tryReserve(licenseId: string, date: string, chunks: number, cap: number): Promise<Reservation>;
  /** Give back a reservation whose batch never happened (the upstream call failed). Best
   * effort: an unreleased reservation only makes the cap stricter for the rest of the day,
   * which is the safe direction. */
  release(licenseId: string, date: string, chunks: number): Promise<void>;
  /** Bind an accepted batch to the licence that created it — the GET route's ownership check
   * (§4.1: a licence may only ever read back its own batches). */
  attachBatch(batchId: string, licenseId: string, date: string, chunks: number): Promise<void>;
  /** The licence that created `batchId`, or undefined for a batch this relay never issued
   * (or one already pruned — batches live ~24h upstream, the mapping a few days). */
  batchOwner(batchId: string): Promise<string | undefined>;
}

/** Today's metering date (UTC). The cap day boundary is UTC on purpose: it needs no per-device
 * timezone and the nightly Dream window never straddles two same-day submissions. */
export function utcDate(now: Date = new Date()): string {
  return now.toISOString().slice(0, 10);
}

/** Counters only — the ledger's whole schema. */
type Ledger = Record<string, Record<string, number>>;

/** batchId → {licence, metering date}. The date is only there so old entries can be pruned. */
type BatchOwners = Record<string, { lic: string; date: string }>;

/** How long a batch→licence mapping outlives its metering date. Anthropic batches expire after
 * 24h; a week covers every legitimate late read with margin. */
const BATCH_OWNER_KEEP_DAYS = 7;

function pruneOwners(owners: BatchOwners, today: string): BatchOwners {
  const cutoff = new Date(`${today}T00:00:00Z`).getTime() - BATCH_OWNER_KEEP_DAYS * 86_400_000;
  const kept: BatchOwners = {};
  for (const [id, o] of Object.entries(owners)) {
    const t = new Date(`${o.date}T00:00:00Z`).getTime();
    if (Number.isFinite(t) && t >= cutoff) kept[id] = o;
  }
  return kept;
}

function reserveInto(days: Ledger, licenseId: string, date: string, chunks: number, cap: number): boolean {
  const day = (days[date] ??= {});
  const used = day[licenseId] ?? 0;
  if (used + chunks > cap) return false;
  day[licenseId] = used + chunks;
  return true;
}

export class InMemoryUsageStore implements UsageStore {
  private days: Ledger = {};
  private owners: BatchOwners = {};
  /** (date, license, chunks, batchId) tuples, for tests to assert on what was persisted. */
  readonly records: Array<{ date: string; licenseId: string; chunks: number; batchId: string }> = [];

  usedOn(licenseId: string, date: string): Promise<number> {
    return Promise.resolve(this.days[date]?.[licenseId] ?? 0);
  }

  tryReserve(licenseId: string, date: string, chunks: number, cap: number): Promise<Reservation> {
    return Promise.resolve(reserveInto(this.days, licenseId, date, chunks, cap) ? "ok" : "capped");
  }

  release(licenseId: string, date: string, chunks: number): Promise<void> {
    const day = this.days[date];
    if (day) day[licenseId] = Math.max(0, (day[licenseId] ?? 0) - chunks);
    return Promise.resolve();
  }

  attachBatch(batchId: string, licenseId: string, date: string, chunks: number): Promise<void> {
    this.owners[batchId] = { lic: licenseId, date };
    this.records.push({ date, licenseId, chunks, batchId });
    return Promise.resolve();
  }

  batchOwner(batchId: string): Promise<string | undefined> {
    return Promise.resolve(this.owners[batchId]?.lic);
  }
}

/** On-disk shape v2: counters + batch ownership. v1 files (a bare date→licence→count map) are
 * migrated on read — their batches simply have no recorded owner, which fails closed (404). */
interface LedgerFile {
  days: Ledger;
  batches: BatchOwners;
}

/** Thrown by the file store when the ledger exists but cannot be read or parsed. Never swallowed
 * into "start clean": a corrupt ledger that resets every licence's counter to zero removes the
 * ONLY spend control on the operator's Anthropic key. */
class LedgerUnavailable extends Error {}

export class JsonFileUsageStore implements UsageStore {
  /** Serialises every read-modify-write. Reservation, release and attach all run inside this
   * queue, so the check-and-increment that enforces the cap is atomic for this process. */
  private queue: Promise<unknown> = Promise.resolve();

  constructor(private readonly path: string) {}

  private async load(): Promise<LedgerFile> {
    let raw: string;
    try {
      raw = await readFile(this.path, "utf8");
    } catch (e) {
      // Absent is the first-run case and genuinely means "nothing spent yet".
      if ((e as NodeJS.ErrnoException).code === "ENOENT") return { days: {}, batches: {} };
      throw new LedgerUnavailable("usage ledger unreadable");
    }
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      throw new LedgerUnavailable("usage ledger corrupt");
    }
    if (typeof parsed !== "object" || parsed === null) throw new LedgerUnavailable("usage ledger malformed");
    const o = parsed as Record<string, unknown>;
    if (typeof o.days === "object" && o.days !== null) {
      return {
        days: o.days as Ledger,
        batches: (typeof o.batches === "object" && o.batches !== null ? o.batches : {}) as BatchOwners,
      };
    }
    // Legacy v1 file: the whole object is the days map.
    return { days: parsed as Ledger, batches: {} };
  }

  private async save(file: LedgerFile): Promise<void> {
    await mkdir(dirname(this.path), { recursive: true });
    const tmp = join(dirname(this.path), `.usage.${process.pid}.${Date.now()}.tmp`);
    await writeFile(tmp, JSON.stringify(file), "utf8");
    await rename(tmp, this.path);
  }

  /** Run `f` as the next step of the serialised queue. The queue survives a rejected step. */
  private enqueue<T>(f: (file: LedgerFile) => Promise<T>): Promise<T> {
    const next = this.queue.then(async () => {
      const file = await this.load();
      return f(file);
    });
    this.queue = next.catch(() => undefined);
    return next;
  }

  async usedOn(licenseId: string, date: string): Promise<number> {
    const file = await this.load();
    return file.days[date]?.[licenseId] ?? 0;
  }

  async batchOwner(batchId: string): Promise<string | undefined> {
    const file = await this.load();
    return file.batches[batchId]?.lic;
  }

  tryReserve(licenseId: string, date: string, chunks: number, cap: number): Promise<Reservation> {
    return this.enqueue(async (file) => {
      if (!reserveInto(file.days, licenseId, date, chunks, cap)) return "capped" as const;
      await this.save(file);
      return "ok" as const;
    }).catch((e: unknown) => {
      // Fail closed: an unreadable ledger means the cap cannot be enforced, so nothing is spent
      // until an operator looks at it.
      if (e instanceof LedgerUnavailable) return "unavailable" as const;
      throw e;
    });
  }

  release(licenseId: string, date: string, chunks: number): Promise<void> {
    return this.enqueue(async (file) => {
      const day = file.days[date];
      if (!day) return;
      day[licenseId] = Math.max(0, (day[licenseId] ?? 0) - chunks);
      await this.save(file);
    }).catch(() => undefined); // best effort — an unreleased reservation only tightens the cap
  }

  attachBatch(batchId: string, licenseId: string, date: string, _chunks: number): Promise<void> {
    return this.enqueue(async (file) => {
      file.batches[batchId] = { lic: licenseId, date };
      file.batches = pruneOwners(file.batches, date);
      await this.save(file);
    });
  }
}
