# Deploying the ShogunAI website to Cloudflare

> **Automated:** pushing to **main** runs `.github/workflows/deploy.yml`
> (install → migrate → build → deploy). main is the only branch that deploys to
> production — ship LP changes through a PR into main. A manual
> `workflow_dispatch` on another ref stops at a guard step unless you tick
> `allow_non_main`, which is there for emergencies only. The manual steps below
> are a fallback.

The site is Next.js 16 (App Router, SSR + Node.js-runtime API routes) talking to
**Supabase Postgres** via `postgres.js`. Cloudflare can't run Next SSR natively, so
we use the **OpenNext Cloudflare adapter** (`@opennextjs/cloudflare`) to build a
Cloudflare **Worker** with static assets.

This project currently pins production builds to `next build --webpack`. On
July 22, 2026, local and CI validation showed Turbopack panicking when the
workspace path contains the non-ASCII folder name `ShogunAIα`; webpack avoids
that path-encoding bug and is the safer production choice here.

## One-time setup

1. Install deps (from the repo root):
   ```bash
   pnpm install
   ```
2. Authenticate wrangler with your Cloudflare account (interactive — run locally):
   ```bash
   pnpm --filter @shogun-ai/website exec wrangler login
   ```

## Local preview (Workers runtime)

```bash
pnpm --filter @shogun-ai/website cf:preview
```

This runs `opennextjs-cloudflare build` then serves the Worker locally in `workerd`,
so you see exactly what Cloudflare will run. Put local secrets in
`apps/website/.dev.vars` (git-ignored):

```
DATABASE_URL=postgres://postgres.<ref>:<password>@aws-0-<region>.pooler.supabase.com:5432/postgres
```

## Cloudflare build & deploy commands (monorepo — important)

This is a pnpm **workspace**. Cloudflare installs from the repo root, so the build and
deploy commands must explicitly target `apps/website`, otherwise `wrangler deploy`
runs at the workspace root and fails with *"application detection logic has been run
in the root of a workspace."*

Set these in the Cloudflare dashboard (Workers & Pages → your project → Settings →
Build):

| Setting | Value |
| --- | --- |
| **Build command** | `pnpm --filter @shogun-ai/website cf:build` |
| **Deploy command** | `pnpm --filter @shogun-ai/website exec wrangler deploy --env preview` |
| **Root directory** | leave as repo root (the `--filter` handles the rest) |

> `--env preview` is load-bearing: `shogunaios.com` and `www.shogunaios.com` are
> custom domains of the `preview` environment in `wrangler.jsonc` (worker
> `shogunai-website-codex-preview`), which also holds the D1 binding and
> `WAITLIST_ALLOWED_ORIGINS`. A bare `wrangler deploy` ships the top-level
> `shogunai-website` worker, which no domain routes to — the deploy succeeds and
> production does not change. Drop the flag only after moving the domains back
> to the top-level worker.

> Do **not** use `pnpm run build` (that only runs `next build`, so `.open-next/worker.js`
> is never produced) and do **not** use a bare `npx wrangler deploy` from the root.
>
> Alternative: set **Root directory = `apps/website`**, then build `pnpm cf:build` and
> deploy `npx wrangler deploy --env preview`. Both approaches are equivalent.

Both commands were validated locally: `cf:build` produces `.open-next/worker.js`, and
`wrangler deploy --dry-run` packages a ~2.5 MB (gzip) Worker with the `ASSETS` binding.

## Preview environments per branch (what you asked for)

Once connected via **Workers & Pages → Create → Workers → Connect to Git** with the
commands above, Cloudflare builds **every branch/PR** and gives each a **preview URL** —
so you can review changes live before promoting to production.

The LP renders fine in preview **without a database**: `page.tsx` and the invite
lookup are wrapped in try/catch and degrade gracefully. The counter falls back to
the established public count (`468`) until `DATABASE_URL` is configured.

## Secrets / environment variables

Set these per-environment (dashboard → Settings → Variables, or `wrangler secret put`):

| Name | Where | Value |
| --- | --- | --- |
| `DATABASE_URL` | prod + preview | Supabase **session pooler** connection string (port `5432`, not `6543` — see below). Required for waitlist signup and all billing routes. Still needed with Hyperdrive: it is the fallback, and what migrations run against. |
| `APP_ORIGIN` | prod | e.g. `https://shogunaios.com`. Read at runtime — `NEXT_PUBLIC_APP_ORIGIN` is inlined at build time and the deploy does not set it. |
| `LOGO_DEV_TOKEN` | prod + preview | Logo.dev token. Kept server-side by `/api/brand-logo/*`; never use a `NEXT_PUBLIC_*` variable. |
| `STRIPE_SECRET_KEY` | prod | Live or test — **must match the mode of the Price IDs below.** A mismatch answers `{"reason":"price_mode_mismatch"}` on checkout. |
| `STRIPE_PRICE_{STANDARD,PRO}_{ANNUAL,MONTHLY}` | prod | All four, or `billingReady()` closes billing entirely. |
| `STRIPE_WEBHOOK_SECRET` | prod | From the endpoint's **Signing secret**. Rolling it needs the new value here within the overlap window you chose. |
| `LICENSE_SIGNING_KEY` | prod | base64 of the PKCS#8 PEM from `scripts/gen-license-keypair.mjs`. Without it `/api/license/verify` answers `signing_key_not_configured` — purchases still complete, but no installed Mac can verify. |

**Adding a secret in the dashboard is not enough on its own.** A newly added secret did not
reach `process.env` in the running Worker until the next `wrangler deploy`, while every secret
that predated that deploy kept working — so the symptom is one variable reading as unset while
its neighbours in the same list are fine. Re-run the deploy workflow after adding one, or set it
with `wrangler secret put`, which uploads a version itself:

```bash
npx wrangler secret put LICENSE_SIGNING_KEY --name shogunai-website-codex-preview
```

A deploy does **not** clear secrets — wrangler preserves them. Plain-text `vars` are a different
matter: `wrangler.jsonc` replaces those on every deploy, so anything set as **Text** in the
dashboard is lost at the next push. Use **Secret** for anything that must survive.

## Database from Workers (Supabase)

`postgres.js` needs a TCP connection. Two options:

1. **Simplest:** set `DATABASE_URL` to the Supabase **session pooler** URL — host
   `aws-0-<region>.pooler.supabase.com`, port **`5432`**. With `nodejs_compat` enabled
   (already in `wrangler.jsonc`) the Worker can open the TCP socket directly.

   Use the **session** pooler, not the transaction pooler on `6543`. `src/db/index.ts`
   creates the client without `prepare: false`, so postgres.js uses prepared statements —
   which the transaction pooler does not support. Pointing `DATABASE_URL` at `6543`
   connects and authenticates fine, then fails once queries start, which is the most
   expensive way to be wrong. Switch to `6543` only together with `prepare: false`.
2. **Hyperdrive (what production uses).** The shim in option 1 does not hold up: roughly half
   of all production database calls came back `proxy request failed, cannot connect to the
   specified address`, and postgres.js surfaces that asynchronously, so it escaped the routes'
   error handling and killed whole requests. Hyperdrive terminates the connection inside
   Cloudflare's network instead.

   The config is bound as `HYPERDRIVE` in `wrangler.jsonc`. `src/db/index.ts` prefers the
   binding and falls back to `DATABASE_URL` wherever the binding is absent — `next dev`, the
   test suites, `db:migrate` — so `DATABASE_URL` is still required, and is still what
   migrations run against.

   Two things to keep right when re-creating the config:

   - **Connection type: public.** Supabase's pooler is a public endpoint; the private options
     are for databases behind a Cloudflare Tunnel or Workers VPC.
   - **Query caching: off.** This database holds licence validity and subscription status. A
     cached read means a revoked licence keeps verifying until the entry expires. What we want
     from Hyperdrive is the connection, not the cache.

   **Hyperdrive is not a fix for connection reuse.** After the binding was live and reporting
   zero errors, half of all licence reads still failed — because the client was cached at
   module scope, and Cloudflare's rule applies to Hyperdrive too:

   > TCP sockets cannot be created in global scope and shared across requests. You should
   > always create TCP sockets within a handler.

   The first request in an isolate opened the connection and worked; every later request in
   that isolate inherited a socket owned by a finished request and hung until its deadline.
   `src/db/index.ts` now keys clients by the per-request Cloudflare context in a `WeakMap`.
   **Never hoist that client back to module scope**, whatever the transport.

   Two things made this hard to see, both worth remembering:

   - The Hyperdrive dashboard showed 48 queries and **0 errors** while most requests failed.
     A request that hangs on a dead local socket never reaches Hyperdrive, so a healthy
     dashboard is not evidence that the database layer is healthy.
   - The symptom looks random from the outside (~50%, no pattern by key or by route). It is
     not random: it is a function of whether the isolate serving you is cold. The log line
     that fires **once per isolate** is what made that visible — lining its presence up
     against the response status separated the two populations exactly, 20/20 and 20/20.

## Running migrations

Migrations run against Supabase directly (not from the Worker):

```bash
DATABASE_URL=... pnpm --filter @shogun-ai/website db:migrate
```

## Deploy to production

```bash
pnpm --filter @shogun-ai/website cf:build
pnpm --filter @shogun-ai/website exec wrangler deploy --env preview
```

`cf:deploy` builds and deploys in one step but targets the default environment,
so it does not touch the domain-owning worker — use the two commands above.
