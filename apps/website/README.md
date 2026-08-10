# ShogunAI — Web app + Referral Engine

Next.js 16 (App Router, RSC) + React 19 + TypeScript. Hosts the ShogunAI
landing page and a portable **referral / "skip the line"** waitlist engine
implemented per `REFERRAL_ENGINE.md`.

## Stack

- **UI**: Tailwind CSS v4 (`@theme` tokens), shadcn-style primitives on Radix,
  Motion (`motion/react`), Lucide icons, `next/font` (Geist + Inter).
- **Design**: bright "Aside Skyglass" light theme — sky-blue accent `#00A6F4`,
  near-black ink, pill controls, restrained shadows.
- **i18n**: English (default) + Japanese, cookie-based locale with an in-nav
  toggle. All copy lives in `src/i18n/dictionaries.ts` (content separated from
  layout).
- **Content**: MDX blog content collection under `content/blog/` (gray-matter
  + next-mdx-remote). `/blog`, `/careers`, `/about` are wired frames.
- **SEO/LLM**: Metadata API, dynamic metadata, `sitemap.xml`, `robots.txt`,
  JSON-LD (Organization / SoftwareApplication / Article / Breadcrumb), OG +
  Twitter cards, dynamic OG image (`opengraph-image`), `public/llms.txt`.
- **Backend**: Drizzle ORM + PostgreSQL (referral engine, unchanged).

## Structure

```
src/
  app/            routes, api/, sitemap.ts, robots.ts, opengraph-image.tsx
  components/
    ui/           button, card, badge, input (shadcn-style)
    sections/     Nav, Hero, Trust, Memory, Action, How, Stats,
                  Testimonials, Pricing, CTA, Footer
    animations/   Reveal (scroll-triggered motion)
    seo/          JsonLd + schema builders
  i18n/           config, dictionaries (en/ja), server helpers
  lib/            referral, service, http, rate-limit, waitlist-auth, blog, site
  db/             schema, queries, migrate
content/blog/     *.mdx posts
```

## What's here

| Area | Files |
| --- | --- |
| Landing page | `src/app/page.tsx`, `src/app/*.css` |
| Status dashboard | `src/app/status/page.tsx`, `src/components/StatusDashboard.tsx` |
| Core engine (pure) | `src/lib/referral.ts` — tokens, tiers, masking, validation |
| Signup / qualifying action | `src/lib/service.ts` |
| Data layer (SQL) | `src/db/queries.ts`, `src/db/schema.ts` |
| Security | `src/lib/{http,rate-limit,waitlist-auth}.ts` |
| API routes | `src/app/api/waitlist/{signup,profile,status,leaderboard,invite-context}` |

## The two-token invariant

Every participant carries a **public** `ref_code` (broadcast in share links,
grants attribution only) and a **private** `status_token` (bearer for reading
own status / writing answers). A short public code fails the status-token
regex, so it can never be used as a bearer. Never blur these — see
`REFERRAL_ENGINE.md §1`.

## Setup

```bash
cp .env.example .env.local          # then edit values
npm install
npm run db:migrate                  # creates tables (idempotent)
npm run dev                         # http://localhost:3000
```

Requires a reachable PostgreSQL (`DATABASE_URL`). For production migrations
prefer `npm run db:generate` (drizzle-kit) + versioned SQL.

## Verify

```bash
npm run typecheck   # app source, strict
npm test            # pure-lib unit tests (node:test)
npm run e2e         # drives signup→qualify→count/rank/leaderboard on real PG
```

`npm run e2e` asserts the full flow plus the security edges: two-token split,
self-referral drop, invalid-ref drop, single-fire qualification, no
double-count, and the distinct-IP-hash fraud signal.

## API

| Method + path | Auth | Returns |
| --- | --- | --- |
| `POST /api/waitlist/signup` | origin allowlist + rate limit + honeypot, or `x-webhook-secret` | `{ ok, refCode, statusUrl }` |
| `POST /api/waitlist/profile` | private status token in body | `{ ok, qualified, justQualified }` |
| `GET /api/waitlist/status?code=` | private status token | dashboard payload (no email) |
| `POST /api/stripe/checkout` | origin rate limit | `{ ok, url }` — Stripe Checkout session |
| `POST /api/stripe/portal` | licence key in body | `{ ok, url }` — Stripe Customer Portal |
| `POST /api/stripe/webhook` | **Stripe signature (mandatory)** | `{ ok }` |
| `POST /api/license/verify` | licence key + device id in body | plan, status, period end, signed token |

## Billing (issue #8)

Stripe Checkout + Billing, mirrored into `subscriptions` / `licenses` by the
webhook so "who can use the app until when" is one SELECT away. Prices live in
`src/lib/pricing.ts` (Standard $49/mo billed annually or $62 monthly; Pro
$99/mo billed annually or $124 monthly) and must match the LP copy in
`src/i18n/dictionaries.ts` — `tests/billing.test.ts` fails if they drift.

Price IDs never reach the browser: the client sends a plan name and the server
resolves the Stripe price from `STRIPE_PRICE_*`. With `STRIPE_SECRET_KEY`
unset every billing route answers 503 and the site keeps working.

```bash
node ../../scripts/gen-license-keypair.mjs   # LICENSE_SIGNING_KEY + public key
stripe listen --forward-to localhost:3000/api/stripe/webhook   # local webhook
```

The public key from that script goes into `EMBEDDED_PUBLIC_KEY_B64` in
`crates/shogun-license/src/lib.rs` so shipped Macs can verify licence tokens
offline. Full design: `docs/fixes/2026-08-10-stripe-billing-flow-design.md`.
| `GET /api/waitlist/leaderboard?limit=` | public | masked top-N |
| `GET /api/waitlist/invite-context?ref=` | public | masked inviter + tier |

Errors everywhere: `{ ok:false, error:"<code>" }` with 400/403/404/413/429/500.

## Security invariants (all exercised by tests)

Two-token split · parameterized SQL only · strict email charset · CSV
formula-injection guard · 8 KB body cap · DB-backed per-IP rate limits
(fail-open, `CF-Connecting-IP`) · response minimization (`noindex`, no PII) ·
silent self/invalid-ref drop · referral fraud caught at reward time via salted
IP hashes. See `REFERRAL_ENGINE.md §6`.
