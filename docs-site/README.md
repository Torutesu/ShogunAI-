# docs-site — public product documentation

The Mintlify source for SHOGUN's public docs. This is the **foundation only**:
config, theme, navigation, and a first pass at the pages that already have a
stable surface (Memory API, permissions, privacy). Deepening the content is
follow-up work.

Internal design docs, specs and runbooks stay in `docs/` and are **not**
published here.

## Layout

```
docs-site/
  docs.json            # Mintlify config: theme, colors, navigation
  favicon.png
  logo/                # light.svg (brand blue), dark.svg (white)
  introduction.mdx
  quickstart.mdx
  concepts/            # memory, agents (L1/L2/L3), privacy
  memory-api/          # overview, mcp, cli, rest
```

## Preview locally

```bash
npm i -g mint     # once
cd docs-site
mint dev          # http://localhost:3000
```

`mint broken-links` checks internal links before you push.

## Connecting the repo (one-time, dashboard side)

1. Sign in to Mintlify with the Select account.
2. Install the Mintlify GitHub app on `Torutesu/ShogunAI-`.
3. Set the **deployment branch** to `main` and the **content directory** to
   `docs-site`.
4. Point the custom domain at `docs.syogun.com` and add the CNAME record the
   dashboard shows.

After that, a merge to `main` that touches `docs-site/` deploys.

## House rules for these pages

- Public product documentation only. If a page would leak an unshipped
  decision, an internal runbook, or a schema in flight, it does not belong.
- Never document a claim the product does not keep — the privacy page in
  particular is a promise, not a pitch.
- English, in the product's voice: state things, do not hedge. No emoji.
- Brand blue `#004CFC` is the accent. Don't introduce a second one.
