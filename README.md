# ShogunAI

Monorepo for **ShogunAI** — the operating system for the AI-native individual.
Memory that captures your day. Execution that acts on it.

## Structure

```
shogun-ai/
├── apps/
│   ├── website/     # Marketing site + blog + waitlist/referral engine (Next.js 16)
│   ├── desktop/     # macOS app — Memory + Execution layers (scaffold)
│   └── api/         # Standalone API (scaffold; API currently lives in website)
├── packages/
│   ├── ui/          # Shared UI primitives (scaffold)
│   ├── types/       # Shared domain types
│   ├── utils/       # Shared framework-agnostic utilities
│   ├── config/      # Shared tsconfig base
│   └── shared/      # Shared domain logic (referral engine target)
├── pnpm-workspace.yaml
├── turbo.json
└── package.json
```

## Getting started

```bash
pnpm install
pnpm dev            # turbo run dev across apps
pnpm build          # turbo run build
pnpm web:dev        # just the website
```

The website needs a PostgreSQL `DATABASE_URL` (see `apps/website/.env.example`).

## Apps

- **website** — the live site: bright "Aside Skyglass" theme with light/dark
  toggle, four languages (EN/JA/ES/DE), MDX blog, SEO/LLM tooling, and a
  portable referral/waitlist engine (Drizzle + Postgres). See
  `apps/website/README.md`.
- **desktop** / **api** — scaffolds for the macOS app and an optional
  standalone API.

## Packages

Shared code lives under `packages/*` and is consumed via `@shogun-ai/*`
workspace imports. They start as skeletons; app code migrates in incrementally
(the pure referral engine in `apps/website/src/lib/referral.ts` is the first
candidate for `@shogun-ai/shared`).
