# Deploying the ShogunAI website to Cloudflare

> **GitHub Actions is the only way this site gets deployed.** Pushing to the
> release branch runs `.github/workflows/deploy.yml` (install → migrate → build
> → deploy). Do not run `wrangler deploy` from a laptop or an agent workspace.
>
> There is one Worker, `shogunai-website`, and no preview environment, so two
> deployers means last-writer-wins. That is not hypothetical: on 2026-07-25 a
> direct `wrangler deploy` replaced the shipped LP with an older parallel build,
> and neither side noticed until the live page was inspected. `pnpm cf:deploy`
> now refuses to run outside Actions (`scripts/assert-ci-deploy.mjs`).
>
> The script guard is a speed bump, not a lock. The control that actually holds
> is credentials: keep `CLOUDFLARE_API_TOKEN` in GitHub Actions secrets and
> nowhere else, so a local deploy has nothing to authenticate with. If a token
> has been used locally, rotate it.

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
# WAITLIST_LIVE_COUNT is intentionally left unset locally, so the counter shows "468+".
```

## Do not connect the Cloudflare dashboard to Git

Earlier revisions of this file described wiring **Workers & Pages → Connect to
Git** with dashboard build/deploy commands. Do not do that, and disconnect it if
it is already set up.

A dashboard Git integration is a *second deployer* pointed at the same Worker.
It would build branches Actions doesn't, on its own schedule, and overwrite
production without anything in this repo recording that it happened. That is the
exact failure this setup is now designed to prevent.

Monorepo note, kept because the workflow depends on it: this is a pnpm
workspace, so build and deploy must target `apps/website` explicitly
(`pnpm --filter @shogun-ai/website ...`). A bare `wrangler deploy` at the
workspace root fails with *"application detection logic has been run in the root
of a workspace."*

## Previewing a branch

There is no per-branch preview URL, because there is one Worker and one
deployer. To review a branch before it ships:

- `pnpm --filter @shogun-ai/website cf:preview` runs the real OpenNext output in
  `workerd` locally — the closest thing to production without deploying.
- `pnpm --filter @shogun-ai/website dev` is enough for most visual review.

The LP renders fine **without a database**: `page.tsx` and the invite lookup are
wrapped in try/catch and degrade gracefully, and the "468+" counter is static
unless `WAITLIST_LIVE_COUNT=true`. So a visual check needs no DB.

If per-branch previews become worth having, add them as a **separate Worker**
(e.g. `shogunai-website-preview`) deployed by its own Actions job — never by
giving a second system write access to the production Worker.

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

Merge to the release branch. `.github/workflows/deploy.yml` does the rest, and
the run log ends with the live URL and a version id.

For a deploy that isn't tied to a new commit — a rollback, or re-running after a
transient failure — use **Actions → Deploy website (Cloudflare) → Run workflow**
and pick the ref. That still runs in CI, so the deploy stays attributable.

`pnpm --filter @shogun-ai/website cf:deploy` exists for the workflow to call and
exits non-zero anywhere else. To check a Workers build locally, use `cf:preview`
— it runs the same OpenNext output in `workerd` without touching production.
