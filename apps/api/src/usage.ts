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
  /** Record an accepted batch: `chunks` more for `licenseId` on `date`. */
  record(licenseId: string, date: string, chunks: number, batchId: string): Promise<void>;
}

/** Today's metering date (UTC). The cap day boundary is UTC on purpose: it needs no per-device
 * timezone and the nightly Dream window never straddles two same-day submissions. */
export function utcDate(now: Date = new Date()): string {
  return now.toISOString().slice(0, 10);
}

/** Counters only — the ledger's whole schema. */
type Ledger = Record<string, Record<string, number>>;

export class InMemoryUsageStore implements UsageStore {
  private ledger: Ledger = {};
  /** (date, license, chunks, batchId) tuples, for tests to assert on what was persisted. */
  readonly records: Array<{ date: string; licenseId: string; chunks: number; batchId: string }> = [];

  usedOn(licenseId: string, date: string): Promise<number> {
    return Promise.resolve(this.ledger[date]?.[licenseId] ?? 0);
  }

  record(licenseId: string, date: string, chunks: number, batchId: string): Promise<void> {
    const day = (this.ledger[date] ??= {});
    day[licenseId] = (day[licenseId] ?? 0) + chunks;
    this.records.push({ date, licenseId, chunks, batchId });
    return Promise.resolve();
  }
}

export class JsonFileUsageStore implements UsageStore {
  private queue: Promise<unknown> = Promise.resolve();

  constructor(private readonly path: string) {}

  private async load(): Promise<Ledger> {
    try {
      const raw = await readFile(this.path, "utf8");
      const parsed: unknown = JSON.parse(raw);
      if (typeof parsed === "object" && parsed !== null) return parsed as Ledger;
      return {};
    } catch {
      return {}; // absent or corrupt → start clean; the cap fails open per §4.5, never the app
    }
  }

  async usedOn(licenseId: string, date: string): Promise<number> {
    const ledger = await this.load();
    return ledger[date]?.[licenseId] ?? 0;
  }

  record(licenseId: string, date: string, chunks: number, _batchId: string): Promise<void> {
    // Serialise writers through a promise queue: last-write-wins on the whole file would drop
    // counts under concurrent submissions.
    const next = this.queue.then(async () => {
      const ledger = await this.load();
      const day = (ledger[date] ??= {});
      day[licenseId] = (day[licenseId] ?? 0) + chunks;
      await mkdir(dirname(this.path), { recursive: true });
      const tmp = join(dirname(this.path), `.usage.${process.pid}.${Date.now()}.tmp`);
      await writeFile(tmp, JSON.stringify(ledger), "utf8");
      await rename(tmp, this.path);
    });
    this.queue = next.catch(() => undefined);
    return next;
  }
}
