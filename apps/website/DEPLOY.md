# Deploying the ShogunAI website to Cloudflare

> **Automated:** pushing to the branch runs `.github/workflows/deploy.yml`
> (install → migrate → build → deploy). The manual steps below are a fallback.

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
DATABASE_URL=postgres://...supabase-session-pooler...:6543/postgres
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
| `DATABASE_URL` | prod + preview | Supabase **session pooler** connection string. Required for live count and waitlist signup. |
| `NEXT_PUBLIC_APP_ORIGIN` | prod | e.g. `https://shogunai.com` |
| `LOGO_DEV_TOKEN` | prod + preview | Logo.dev token. Kept server-side by `/api/brand-logo/*`; never use a `NEXT_PUBLIC_*` variable. |

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
pnpm --filter @shogun-ai/website cf:build
pnpm --filter @shogun-ai/website exec wrangler deploy --env preview
```

`cf:deploy` builds and deploys in one step but targets the default environment,
so it does not touch the domain-owning worker — use the two commands above.
