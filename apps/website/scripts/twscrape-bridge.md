# X snapshot bridge (twscrape) + social-points sync

The award engine only ever reads the `x_follower_snapshot` / `x_quote_snapshot`
tables (see `src/lib/points.ts` → `computeSocialAwards`). Populating them is a
swappable **source** (`src/lib/xsource.ts`):

- `httpXSource()` — calls a small HTTP bridge you run (reuse the LEADSHOGUN
  twscrape + Supabase setup). **This is the initial path.**
- `twscrapeSource` — stub; a placeholder that throws until wired.
- Later: an official-X-API source. Nothing downstream changes.

## The bridge contract

Run a tiny service (Python + twscrape, on Fly/Render/a box) exposing:

```
GET {X_BRIDGE_URL}/followers?account=<handle>
    -> { "handles": ["alice", "bob", ...] }

GET {X_BRIDGE_URL}/quotes?tweet_id=<launch tweet id>
    -> { "quotes": [ { "authorHandle": "alice", "quoteTweetId": "123", "text": "..." } ] }
```

Protect it with a bearer: the Worker sends `Authorization: Bearer <X_BRIDGE_TOKEN>`.

### Reference (twscrape, pseudo)

```python
# followers
accounts = api.followers(user_id)            # twscrape
return {"handles": [a.username.lower() for a in accounts]}

# quotes of the launch tweet
qs = api.tweet_replies_or_quotes(tweet_id)   # twscrape
return {"quotes": [
    {"authorHandle": q.user.username.lower(), "quoteTweetId": str(q.id), "text": q.rawContent}
    for q in qs
]}
```

The batch-pull design (spec §3.1) means API calls scale with the number of
**snapshots**, not participants.

## Environment (set as Worker secrets)

| Var | Purpose |
| --- | --- |
| `X_PRODUCT_ACCOUNT` | product X handle whose followers earn `follow_product` |
| `X_FOUNDER_ACCOUNT` | founder X handle → `follow_founder` |
| `X_LAUNCH_TWEET_ID` | the launch tweet whose quotes earn `quote` |
| `X_BRIDGE_URL` | base URL of the twscrape bridge |
| `X_BRIDGE_TOKEN` | shared bearer for the bridge |
| `ADMIN_TOKEN` | ≥16 chars — gates `/admin`, `/api/admin/*` |

## Running the sync

`POST /api/admin/sync-x?key=<ADMIN_TOKEN>` pulls the three snapshots, persists
them, and awards social points idempotently. Returns
`{ product, founder, quotes, awarded }`.

Schedule it (every few hours) with any external cron hitting that endpoint —
e.g. a GitHub Actions workflow:

```yaml
# .github/workflows/sync-x.yml
on:
  schedule: [{ cron: '0 */6 * * *' }]
jobs:
  sync:
    runs-on: ubuntu-latest
    steps:
      - run: curl -fsS -X POST "https://<your-worker>/api/admin/sync-x?key=${{ secrets.ADMIN_TOKEN }}"
```

Gates (spec §3.2/§3.5) are enforced at award time: a quote needs a comment
(`>= 10` chars) **and** `#ad`/`#PR`. Account-age / follower-count gates need
extra columns on `x_quote_snapshot` — a documented follow-up.
