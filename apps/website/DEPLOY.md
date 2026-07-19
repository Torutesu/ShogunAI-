# Deploying the ShogunAI website to Cloudflare

> **Automated:** pushing to the branch runs `.github/workflows/deploy.yml`
> (install → migrate → build → deploy). The manual steps below are a fallback.

The site is Next.js 16 (App Router, SSR + Node.js-runtime API routes) talking to
**Supabase Postgres** via `postgres.js`. Cloudflare can't run Next SSR natively, so
we use the **OpenNext Cloudflare adapter** (`@opennextjs/cloudflare`) to build a
Cloudflare **Worker** with static assets.

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
DATABASE_URL=postgres://...supabase-session-pooler...:6543/postgres
# WAITLIST_LIVE_COUNT is intentionally left unset locally, so the counter shows "468+".
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
| **Deploy command** | `pnpm --filter @shogun-ai/website exec wrangler deploy` |
| **Root directory** | leave as repo root (the `--filter` handles the rest) |

> Do **not** use `pnpm run build` (that only runs `next build`, so `.open-next/worker.js`
> is never produced) and do **not** use a bare `npx wrangler deploy` from the root.
>
> Alternative: set **Root directory = `apps/website`**, then build `pnpm cf:build` and
> deploy `npx wrangler deploy`. Both approaches are equivalent.

Both commands were validated locally: `cf:build` produces `.open-next/worker.js`, and
`wrangler deploy --dry-run` packages a ~2.5 MB (gzip) Worker with the `ASSETS` binding.

## Preview environments per branch (what you asked for)

Once connected via **Workers & Pages → Create → Workers → Connect to Git** with the
commands above, Cloudflare builds **every branch/PR** and gives each a **preview URL** —
so you can review changes live before promoting to production.

The LP renders fine in preview **without a database**: `page.tsx` and the invite
lookup are wrapped in try/catch and degrade gracefully, and the "468+" counter is
static unless `WAITLIST_LIVE_COUNT=true`. So a first visual preview needs no DB.

## Secrets / environment variables

Set these per-environment (dashboard → Settings → Variables, or `wrangler secret put`):

| Name | Where | Value |
| --- | --- | --- |
| `DATABASE_URL` | prod + preview (only if you want live data) | Supabase **session pooler** connection string |
| `WAITLIST_LIVE_COUNT` | **production only** | `true` — adds real signups on top of the 468 base. Leave unset everywhere else so dev/preview always read `468+`. |
| `NEXT_PUBLIC_APP_ORIGIN` | prod | e.g. `https://shogunai.com` |

## Database from Workers (Supabase)

`postgres.js` needs a TCP connection. Two options:

1. **Simplest:** set `DATABASE_URL` to the Supabase **session pooler** URL (port
   `6543`). With `nodejs_compat` enabled (already in `wrangler.jsonc`) the Worker can
   open the TCP socket directly.
2. **Lower latency (recommended for prod):** create a **Cloudflare Hyperdrive** config
   pointing at the same Supabase database, bind it in `wrangler.jsonc`, and set
   `DATABASE_URL` to the Hyperdrive connection string. No app code change is needed —
   `src/db/index.ts` just reads `DATABASE_URL`.

## Running migrations

Migrations run against Supabase directly (not from the Worker):

```bash
DATABASE_URL=... pnpm --filter @shogun-ai/website db:migrate
```

## Deploy to production

```bash
pnpm --filter @shogun-ai/website cf:deploy
```
