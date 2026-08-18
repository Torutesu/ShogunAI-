import postgres from 'postgres';

/**
 * Idempotent schema bootstrap. Run with `npm run db:migrate`.
 * For real deployments prefer `drizzle-kit generate` + versioned migrations;
 * this exists so the engine can be driven end-to-end with one command.
 */
const url =
  process.env.DATABASE_URL ??
  'postgres://postgres:postgres@localhost:5432/shogun_waitlist';

const sql = postgres(url, { max: 1 });

async function main() {
  await sql`CREATE EXTENSION IF NOT EXISTS pgcrypto`;

  await sql`
    CREATE TABLE IF NOT EXISTS participants (
      id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
      email          text NOT NULL UNIQUE,
      created_at     timestamptz NOT NULL DEFAULT now(),
      status         text NOT NULL DEFAULT 'pending',
      ref_code       text UNIQUE,
      status_token   text UNIQUE,
      referred_by    text,
      qualified_at   timestamptz,
      ip_hash        text,
      nickname       text,
      answer_1       text,
      answer_2       text,
      answer_3       text
    )
  `;
  // Additive for existing databases.
  await sql`ALTER TABLE participants ADD COLUMN IF NOT EXISTS nickname text`;
  await sql`ALTER TABLE participants ADD COLUMN IF NOT EXISTS join_position integer`;
  await sql`ALTER TABLE participants ADD COLUMN IF NOT EXISTS x_handle text`;
  await sql`ALTER TABLE participants ADD COLUMN IF NOT EXISTS plan text`;
  await sql`CREATE INDEX IF NOT EXISTS participants_ref_code_idx     ON participants (ref_code)`;
  await sql`CREATE INDEX IF NOT EXISTS participants_status_token_idx ON participants (status_token)`;
  await sql`CREATE INDEX IF NOT EXISTS participants_referred_by_idx  ON participants (referred_by)`;
  await sql`CREATE UNIQUE INDEX IF NOT EXISTS participants_x_handle_key ON participants (x_handle)`;

  await sql`
    CREATE TABLE IF NOT EXISTS rate_limits (
      key          text PRIMARY KEY,
      window_start timestamptz NOT NULL,
      count        integer NOT NULL DEFAULT 0
    )
  `;

  // --- Gamification: points ledger + X snapshots (SHOGUN waitlist spec §3.4) ---
  await sql`
    CREATE TABLE IF NOT EXISTS points_ledger (
      id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
      entry_id    uuid NOT NULL REFERENCES participants(id) ON DELETE CASCADE,
      action_type text NOT NULL,
      points      integer NOT NULL,
      source_ref  text NOT NULL DEFAULT '',
      awarded_at  timestamptz NOT NULL DEFAULT now(),
      CONSTRAINT points_ledger_dedup UNIQUE (entry_id, action_type, source_ref)
    )
  `;
  await sql`CREATE INDEX IF NOT EXISTS points_ledger_entry_idx ON points_ledger (entry_id)`;

  await sql`
    CREATE TABLE IF NOT EXISTS x_follower_snapshot (
      account     text NOT NULL,
      handle      text NOT NULL,
      snapshot_at timestamptz NOT NULL DEFAULT now(),
      PRIMARY KEY (account, handle, snapshot_at)
    )
  `;
  await sql`
    CREATE TABLE IF NOT EXISTS x_quote_snapshot (
      tweet_id       text NOT NULL,
      author_handle  text NOT NULL,
      quote_tweet_id text NOT NULL,
      text           text NOT NULL DEFAULT '',
      snapshot_at    timestamptz NOT NULL DEFAULT now()
    )
  `;

  // Additive only. Rollback = `DROP TABLE stripe_events, licenses, subscriptions,
  // billing_customers;` — nothing outside billing references these tables, and the waitlist
  // engine keeps working without them.
  await sql`
    CREATE TABLE IF NOT EXISTS billing_customers (
      id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
      email              text NOT NULL UNIQUE,
      stripe_customer_id text NOT NULL UNIQUE,
      created_at         timestamptz NOT NULL DEFAULT now()
    )
  `;
  await sql`CREATE INDEX IF NOT EXISTS billing_customers_stripe_idx ON billing_customers (stripe_customer_id)`;

  await sql`
    CREATE TABLE IF NOT EXISTS subscriptions (
      id                     uuid PRIMARY KEY DEFAULT gen_random_uuid(),
      stripe_subscription_id text NOT NULL UNIQUE,
      stripe_customer_id     text NOT NULL,
      stripe_price_id        text,
      plan                   text,
      interval               text,
      status                 text NOT NULL,
      current_period_start   timestamptz,
      current_period_end     timestamptz,
      cancel_at              timestamptz,
      canceled_at            timestamptz,
      cancel_at_period_end   boolean NOT NULL DEFAULT false,
      trial_end              timestamptz,
      updated_at             timestamptz NOT NULL DEFAULT now()
    )
  `;
  await sql`CREATE INDEX IF NOT EXISTS subscriptions_customer_idx ON subscriptions (stripe_customer_id)`;
  await sql`CREATE INDEX IF NOT EXISTS subscriptions_status_idx   ON subscriptions (status)`;

  await sql`
    CREATE TABLE IF NOT EXISTS licenses (
      id                     uuid PRIMARY KEY DEFAULT gen_random_uuid(),
      license_key            text NOT NULL UNIQUE,
      stripe_customer_id     text NOT NULL,
      stripe_subscription_id text NOT NULL UNIQUE,
      email                  text,
      last_device_id         text,
      last_app_version       text,
      last_verified_at       timestamptz,
      device_count           integer NOT NULL DEFAULT 0,
      revoked_at             timestamptz,
      created_at             timestamptz NOT NULL DEFAULT now()
    )
  `;
  await sql`CREATE INDEX IF NOT EXISTS licenses_subscription_idx ON licenses (stripe_subscription_id)`;
  await sql`CREATE INDEX IF NOT EXISTS licenses_customer_idx     ON licenses (stripe_customer_id)`;

  await sql`
    CREATE TABLE IF NOT EXISTS stripe_events (
      id          text PRIMARY KEY,
      type        text NOT NULL,
      received_at timestamptz NOT NULL DEFAULT now()
    )
  `;

  console.log(
    'migrated: participants, rate_limits, points_ledger, x_follower_snapshot, x_quote_snapshot, ' +
      'billing_customers, subscriptions, licenses, stripe_events',
  );
  await sql.end();
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
