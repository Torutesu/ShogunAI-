# ShogunAI website

Production website and early-access infrastructure for ShogunAI. Built with
Next.js 16, React 19, TypeScript, Tailwind CSS 4, and OpenNext for Cloudflare.

## What is live

- Product and editorial pages in English, Japanese, Spanish, and German
- Feature, use-case, integration, security, pricing, and comparison pages
- MDX blog, RSS, sitemap, robots directives, JSON-LD, Open Graph assets, and
  `llms.txt` / `llms-full.txt`
- Waitlist signup, private participant status, qualifying profile flow,
  leaderboard, invite context, and referral attribution
- Cloudflare D1 fallback storage for waitlist email capture when the primary
  participant service is unavailable

The macOS product source is maintained outside this website app. The website
describes the production product and owns its public acquisition surfaces.

## Structure

```text
src/
  app/              routes, localized routes, APIs, sitemap, robots
  components/       page shells, sections, forms, UI primitives, JSON-LD
  i18n/             EN/JA/ES/DE dictionaries and locale helpers
  lib/              content, referral rules, services, security, blog, status
  db/               PostgreSQL schema and queries
content/blog/       localized MDX articles
public/             brand, social, crawler, and static assets
```

## Important routes

| Area | Routes |
| --- | --- |
| Product | `/features`, `/use-cases`, `/integrations`, `/security`, `/pricing` |
| Editorial | `/blog`, `/blog/[slug]`, `/rss.xml` |
| Early access | `/api/waitlist/signup`, `/profile`, `/status`, `/leaderboard`, `/invite-context` |
| Participant status | `/status?code=<private-status-token>` |
| Localization | The product routes above are also available below `/en`, `/ja`, `/es`, and `/de` |

## Runtime and data

The main referral implementation supports PostgreSQL through Drizzle. The
Cloudflare deployment also binds the `WAITLIST_METRICS` D1 database so signup
emails can be retained privately when the upstream participant operation is
unavailable. Public referral codes and private status tokens are separate
credentials and must never be treated as interchangeable.

Configure local values from `.env.example`. Production secrets and bindings
must be managed through the deployment platform; do not commit credentials.

## Development

From the repository root:

```bash
pnpm install
pnpm --filter @shogun-ai/website dev
pnpm --filter @shogun-ai/website build
pnpm --filter @shogun-ai/website test
```

Database commands require a reachable `DATABASE_URL`:

```bash
pnpm --filter @shogun-ai/website db:migrate
pnpm --filter @shogun-ai/website db:generate
```

## Security boundaries

- A public referral code grants attribution only.
- A private status token authorizes access to one participant's status and
  qualifying answers.
- API responses minimize personal data and never return raw email addresses in
  public status or leaderboard payloads.
- Request size, origin, honeypot, token format, and rate-limit checks are part
  of the application boundary.
- Provider availability and storage failure behavior must be tested before
  deployment. Documentation is not evidence of a control by itself.

## Deployment

The production target is Cloudflare Workers via OpenNext. Build and deploy with
the repository's Cloudflare scripts and verify both custom domains, localized
routes, waitlist submission, `sitemap.xml`, and `llms.txt` after release.

The Next.js proxy is used in local/runtime routing but is temporarily excluded
from the OpenNext build because that adapter does not support the proxy entry
point. Always restore the file after the build step.

## Content standard

Public claims must match the production service. Product names may be used in
factual comparison pages, but the site's design and voice are described only
through ShogunAI's own principles and tokens.
