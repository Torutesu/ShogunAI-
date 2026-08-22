import postgres from 'postgres';

/**
 * Idempotent schema bootstrap. Run with `npm run db:migrate`.
 * For real deployments prefer `drizzle-kit generate` + versioned migrations;
 * this exists so the engine can be driven end-to-end with one command.
 *
 * `.trim()` normalises a value pasted into a CI secret. postgres.js happens to tolerate a trailing
 * newline today (verified against postgres:16), so it is defensive rather than a fix; `||` rather
 * than `??` is the part that matters, so a whitespace-only secret falls back to the local default
 * instead of being sent as a connection string.
 */
const url =
  process.env.DATABASE_URL?.trim() ||
  'postgres://postgres:postgres@localhost:5432/shogun_waitlist';

const sql = postgres(url, { max: 1 });

/**
 * The password exactly as it appears in the connection string, before any decoding.
 *
 * Hand-parsed because `new URL` normalises: it re-encodes reserved characters on the way out, so
 * its `password` is not the text a human pasted. The authority ends at the first `/`, and the
 * userinfo at the **last** `@` within it, so a password containing `@` is read whole.
 */
function literalPassword(raw: string): string {
  const authority = raw.slice(raw.indexOf('://') + 3).split('/')[0];
  const at = authority.lastIndexOf('@');
  if (at < 0) return '';
  const userinfo = authority.slice(0, at);
  const colon = userinfo.indexOf(':');
  return colon < 0 ? '' : userinfo.slice(colon + 1);
}

/**
 * What the connection string actually parsed to, with the password removed.
 *
 * A failed migrate blocks the whole deploy, so the log has to answer "which of the five parts is
 * wrong" without anyone having to reproduce it locally — and it has to do that without printing
 * the credential into a public Actions log.
 *
 * One fact per line: Actions redacts any span matching a registered secret, and a single-line
 * summary loses every fact after the redacted one.
 */
function describeUrl(raw: string): string {
  let u: URL;
  try {
    u = new URL(raw);
  } catch {
    return 'DATABASE_URL is not a parseable URL. A raw `/`, `?`, `#` or `%` in the password must be\npercent-encoded; every other character works as-is.';
  }
  const password = decodeURIComponent(u.password);
  const lines = [
    `  user     ${u.username || '(none)'}`,
    `  host     ${u.hostname}`,
    `  port     ${u.port || '(default)'}`,
    `  database ${u.pathname.replace(/^\//, '') || '(none)'}`,
    `  password ${password ? 'non-empty' : 'EMPTY'}`,
  ];
  if (/^\[.*\]$/.test(password)) {
    lines.push('  ^^ that is still the dashboard placeholder — substitute the real password');
  }
  // Catches a failure with no other symptom: a literal `%` followed by two hex digits is a valid
  // escape, so a password containing one is decoded into a *different* string and sent silently.
  // Verified against postgres:16 — `aB3%2Fx9`, `aB3%41x9` and `pa%73s` each authenticate as
  // something else and come back 28P01, indistinguishable from a wrong password.
  //
  // Read off the raw string rather than `u.password`: the URL parser hands back a re-encoded form
  // (a literal `@` in a password comes back as `%40`), so comparing that against its own decoding
  // flags every password containing a reserved character. Reporting only whether an escape was
  // present leaks nothing about the value.
  if (/%[0-9A-Fa-f]{2}/.test(literalPassword(raw))) {
    lines.push(
      '  ^^ this password was percent-DECODED before being sent: the text contains a `%` escape.',
      '     If the password really contains a literal `%`, write it as `%25` in the URL.',
    );
  }
  return lines.join('\n');
}

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
  // Additive: the claim capability that lets a Mac fetch its own key. Rollback =
  // `ALTER TABLE licenses DROP COLUMN claim_nonce_hash, DROP COLUMN claim_expires_at;` — the
  // manual key-entry path keeps working without them.
  await sql`ALTER TABLE licenses ADD COLUMN IF NOT EXISTS claim_nonce_hash text`;
  await sql`ALTER TABLE licenses ADD COLUMN IF NOT EXISTS claim_expires_at timestamptz`;
  await sql`CREATE INDEX IF NOT EXISTS licenses_subscription_idx ON licenses (stripe_subscription_id)`;
  await sql`CREATE INDEX IF NOT EXISTS licenses_customer_idx     ON licenses (stripe_customer_id)`;
  await sql`CREATE INDEX IF NOT EXISTS licenses_claim_idx        ON licenses (claim_nonce_hash)`;

  await sql`
    CREATE TABLE IF NOT EXISTS stripe_events (
      id          text PRIMARY KEY,
      type        text NOT NULL,
      received_at timestamptz NOT NULL DEFAULT now()
    )
  `;

  // Row Level Security on every table this file creates.
  //
  // Nothing here reaches the database through Supabase's PostgREST layer — `db/index.ts` opens a
  // direct connection with postgres.js as a privileged role, and that role is not subject to RLS.
  // So this changes nothing for the app. What it closes is the *other* door: Supabase grants the
  // `anon` and `authenticated` roles access to public tables by default, and the anon key is
  // meant to be public. `licenses` holds licence keys — bearer credentials — and `participants`
  // holds email addresses.
  //
  // Enabled with **no policies**, which is deny-all for every role RLS applies to. That is the
  // intended end state, not an unfinished one: there is no legitimate anon read of these tables,
  // so a policy would be a hole rather than a fix. Supabase's advisor reports this as
  // "RLS Enabled No Policy" — an informational note that assumes you meant to add policies.
  //
  // `ENABLE`, never `FORCE`: FORCE would subject the table owner to RLS too, and the app connects
  // as a role that is very likely the owner. That would lock out the application itself.
  //
  // Rollback = `ALTER TABLE <t> DISABLE ROW LEVEL SECURITY;` per table. Nothing depends on RLS
  // being on, so disabling it restores the previous behaviour exactly.
  for (const table of [
    'participants',
    'rate_limits',
    'points_ledger',
    'x_follower_snapshot',
    'x_quote_snapshot',
    'billing_customers',
    'subscriptions',
    'licenses',
    'stripe_events',
  ]) {
    await sql`ALTER TABLE ${sql(table)} ENABLE ROW LEVEL SECURITY`;
  }

  console.log(
    'migrated: participants, rate_limits, points_ledger, x_follower_snapshot, x_quote_snapshot, ' +
      'billing_customers, subscriptions, licenses, stripe_events (RLS enabled on all)',
  );
  await sql.end();
}

main().catch((err) => {
  console.error(err);
  // 28P01 is the one failure that is never about the schema: the migration never ran, so the
  // deploy that depends on it is blocked by a credential, not by a broken statement.
  if ((err as { code?: string })?.code === '28P01') {
    console.error(
      `\nDATABASE_URL was rejected by the server. It parsed as:\n${describeUrl(url)}\n\n` +
        'Everything above except the password is reported verbatim, so check those first — and\n' +
        'check the host hardest. Against Supabase, 28P01 does NOT mean "wrong password":\n\n' +
        '  * A pooler host that does not host this project answers 28P01. Supavisor will not\n' +
        '    reveal whether a tenant exists, so an unknown tenant and a bad password are the same\n' +
        '    reply. `aws-0-<region>` and `aws-1-<region>` are different fleets on different load\n' +
        '    balancers; a project on one answers 28P01 on the other with a perfect password.\n' +
        '    Compare the host against the string the dashboard shows *today*.\n' +
        '  * The username carries the routing. It must be `postgres.<project-ref>`, not `postgres`.\n' +
        '  * Only then suspect the password itself.\n\n' +
        'What 28P01 does rule out is a percent-encoding mistake: a raw `/`, `?`, `#` or `%` in the\n' +
        'password fails while building the URL ("Invalid URL" / "URI malformed"), never as a\n' +
        'rejected login. Every other character connects unencoded — verified against postgres:16\n' +
        'across 33 punctuation characters. So the password is not mangled; it is either wrong or\n' +
        'being presented to a server that has never heard of this project.',
    );
  }
  process.exit(1);
});
