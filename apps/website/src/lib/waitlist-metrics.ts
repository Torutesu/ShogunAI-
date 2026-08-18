import { getCloudflareContext } from '@opennextjs/cloudflare';

const importedCount = Math.max(0, Number(process.env.WAITLIST_IMPORTED_COUNT ?? 485));

type MetricsStatement = {
  bind: (...values: unknown[]) => MetricsStatement;
  first: <T>() => Promise<T | null>;
  run: () => Promise<unknown>;
};

type MetricsDatabase = {
  prepare: (query: string) => MetricsStatement;
};

function metricsDb(): MetricsDatabase {
  const { env } = getCloudflareContext();
  return (env as unknown as Record<string, unknown>).WAITLIST_METRICS as MetricsDatabase;
}

/**
 * Durable fallback for public email capture when the external Postgres
 * connection is unavailable from the Worker. The email is intentionally kept
 * in the same private D1 binding as the waitlist counter so a DB timeout never
 * makes the public form unusable.
 */
export async function saveWaitlistEmail(email: string): Promise<boolean> {
  const db = metricsDb();
  await db
    .prepare(`
      CREATE TABLE IF NOT EXISTS waitlist_email_capture (
        email TEXT PRIMARY KEY,
        created_at INTEGER NOT NULL DEFAULT (unixepoch())
      )
    `)
    .run();
  const result = await db
    .prepare('INSERT OR IGNORE INTO waitlist_email_capture (email) VALUES (?1)')
    .bind(email.trim().toLowerCase())
    .run() as { meta?: { changes?: number } };
  return Number(result.meta?.changes ?? 0) > 0;
}

/** Read the durable public count. Email capture remains private to the D1 binding. */
export async function getParticipantCount(): Promise<number> {
  const row = await metricsDb()
    .prepare('SELECT value FROM waitlist_metrics WHERE key = ?1')
    .bind('participants')
    .first<{ value: number }>();
  return Number.isFinite(row?.value) ? Number(row?.value) : importedCount;
}

/** Increment only after Supabase accepted a brand-new email address. */
export async function incrementParticipantCount(): Promise<void> {
  await metricsDb()
    .prepare("UPDATE waitlist_metrics SET value = value + 1, updated_at = unixepoch() WHERE key = ?1")
    .bind('participants')
    .run();
}

/** Atomic, Cloudflare-local rate limit for the only public write endpoint. */
export async function consumeSignupAttempt(identifier: string): Promise<number> {
  const row = await metricsDb()
    .prepare(`
      INSERT INTO waitlist_rate_limits (key, window_start, count)
      VALUES (?1, unixepoch(), 1)
      ON CONFLICT(key) DO UPDATE SET
        count = CASE
          WHEN waitlist_rate_limits.window_start <= unixepoch() - ?2 THEN 1
          ELSE waitlist_rate_limits.count + 1
        END,
        window_start = CASE
          WHEN waitlist_rate_limits.window_start <= unixepoch() - ?2 THEN unixepoch()
          ELSE waitlist_rate_limits.window_start
        END
      RETURNING count
    `)
    .bind(`signup:${identifier}`, 60)
    .first<{ count: number }>();

  return Number(row?.count ?? Number.POSITIVE_INFINITY);
}
