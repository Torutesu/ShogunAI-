# ShogunAI

Monorepo for **ShogunAI** — the operating system for the AI-native individual.
Memory that captures your day. Execution that acts on it.

## Structure

```
shogun-ai/
├── crates/                 # Rust workspace — the macOS app's core (see CLAUDE.md)
│   ├── shogun-core/        # Capture, DB ownership, context cache, event bus, LLM lanes, audio/ASR
│   ├── shogun-memory/      # Schema + migrations, three-tier memory, state tables, search
│   ├── shogun-fusion/      # Context Fusion: f(state, screen_ctx, intent) → action
│   ├── shogun-agents/      # L1/L2/L3 execution engine, preset agents
│   ├── shogun-mcp/         # MCP client + server, REST, scope table, Memory API
│   ├── shogun-integrations/# First-layer connectors (official remote MCP) + Composio
│   ├── shogun-license/     # Licence token verification
│   ├── shogun-redact/      # Secret redaction before writes and logs
│   ├── shogun-cli/         # `shogun` command
│   └── spike-harness/      # Phase 0 spike harness (not product code)
├── apps/
│   ├── website/     # Marketing site + blog + email waitlist (Next.js 16)
│   ├── desktop/     # macOS app — Tauri v2 shell for the Rust core (notch panel, full UI,
│   │                #   onboarding, meetings, voice, visual recall)
│   └── api/         # Select KK backend — Batch relay (/v1/batch) + meeting-ASR token mint
├── packages/
│   ├── ui/          # Shared UI primitives (scaffold)
│   ├── types/       # Shared domain types
│   ├── utils/       # Shared framework-agnostic utilities
│   ├── config/      # Shared tsconfig base
│   └── shared/      # Shared domain logic (scaffold)
├── Cargo.toml
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
  toggle, four languages (EN/JA/ES/DE), MDX blog, SEO/LLM tooling, Stripe
  checkout, and an email-only early-access waitlist. The referral programme
  was retired: `/api/waitlist/{rank,status,profile,leaderboard,invite-context}`
  now return 404 and signup stores an address and a timestamp, nothing else.
  See `apps/website/README.md`.
- **desktop** — the macOS app itself: the Tauri v2 shell around the Rust core,
  with the notch panel, full UI, onboarding, meeting overlay, voice, and
  visual recall. Requires macOS 14+ on Apple Silicon to run.
- **api** — the Select KK backend: the Anthropic Batch relay (`/v1/batch`,
  licence-token authenticated) and the short-lived meeting-ASR token mint.
  This is where the company keys live; the desktop binary never embeds one.

## Packages

Shared code lives under `packages/*` and is consumed via `@shogun-ai/*`
workspace imports. They are still skeletons; app code migrates in
incrementally.

## Docs

`CLAUDE.md` holds the non-negotiable rules for the macOS app. The full
requirements are in `docs/requirements-v1.0.md`, per-feature implementation
and test status in `docs/feature-status.csv`, and — when a doc and the code
disagree — start from `docs/spec-implementation-drift-audit.md`.
