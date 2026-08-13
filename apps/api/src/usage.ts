/**
 * Usage metering (docs/batch-relay-design.md §1 step ③ / §3.2).
 *
 * What may persist server-side is EXACTLY the aggregate needed for billing and cap enforcement:
 * license id, UTC date, chunk count, batch id. The interface has no field that could carry chunk
 * content, so a store cannot leak it even by mistake — the same "no text column" trick the
 * device's traceability_log uses.
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

export interface UsageStore {
  /** Chunks already accepted for `licenseId` on `date` (UTC `YYYY-MM-DD`). */
  usedOn(licenseId: string, date: string): Promise<number>;
  /** Record an accepted batch: `chunks` more for `licenseId` on `date`, and remember that
   * `batchId` belongs to `licenseId` (the GET route's ownership check — §4.1: a licence must
   * only ever read back its own batches). */
  record(licenseId: string, date: string, chunks: number, batchId: string): Promise<void>;
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

export class InMemoryUsageStore implements UsageStore {
  private ledger: Ledger = {};
  private owners: BatchOwners = {};
  /** (date, license, chunks, batchId) tuples, for tests to assert on what was persisted. */
  readonly records: Array<{ date: string; licenseId: string; chunks: number; batchId: string }> = [];

  usedOn(licenseId: string, date: string): Promise<number> {
    return Promise.resolve(this.ledger[date]?.[licenseId] ?? 0);
  }

  record(licenseId: string, date: string, chunks: number, batchId: string): Promise<void> {
    const day = (this.ledger[date] ??= {});
    day[licenseId] = (day[licenseId] ?? 0) + chunks;
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

export class JsonFileUsageStore implements UsageStore {
  private queue: Promise<unknown> = Promise.resolve();

  constructor(private readonly path: string) {}

  private async load(): Promise<LedgerFile> {
    try {
      const raw = await readFile(this.path, "utf8");
      const parsed: unknown = JSON.parse(raw);
      if (typeof parsed !== "object" || parsed === null) return { days: {}, batches: {} };
      const o = parsed as Record<string, unknown>;
      if (typeof o.days === "object" && o.days !== null) {
        return {
          days: o.days as Ledger,
          batches: (typeof o.batches === "object" && o.batches !== null ? o.batches : {}) as BatchOwners,
        };
      }
      // Legacy v1 file: the whole object is the days map.
      return { days: parsed as Ledger, batches: {} };
    } catch {
      return { days: {}, batches: {} }; // absent or corrupt → start clean; the cap fails open per §4.5, never the app
    }
  }

  async usedOn(licenseId: string, date: string): Promise<number> {
    const file = await this.load();
    return file.days[date]?.[licenseId] ?? 0;
  }

  async batchOwner(batchId: string): Promise<string | undefined> {
    const file = await this.load();
    return file.batches[batchId]?.lic;
  }

  record(licenseId: string, date: string, chunks: number, batchId: string): Promise<void> {
    // Serialise writers through a promise queue: last-write-wins on the whole file would drop
    // counts under concurrent submissions.
    const next = this.queue.then(async () => {
      const file = await this.load();
      const day = (file.days[date] ??= {});
      day[licenseId] = (day[licenseId] ?? 0) + chunks;
      file.batches[batchId] = { lic: licenseId, date };
      file.batches = pruneOwners(file.batches, date);
      await mkdir(dirname(this.path), { recursive: true });
      const tmp = join(dirname(this.path), `.usage.${process.pid}.${Date.now()}.tmp`);
      await writeFile(tmp, JSON.stringify(file), "utf8");
      await rename(tmp, this.path);
    });
    this.queue = next.catch(() => undefined);
    return next;
  }
}
