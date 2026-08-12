# Memory API — stdio MCP recipes (Cursor & Claude Desktop)

SHOGUN’s Memory API is exposed three ways (invariant 6): **MCP** (`shogun-mcp` stdio), **REST** (`shogun-api`), and **CLI** (`shogun`). This doc covers the **stdio MCP** face for AI clients.

## Prerequisites

1. **Enable Memory API** in the desktop app: Settings → **Memory API** → On.  
   Fail closed: if `memory_api.json` is missing or `"enabled": false`, `shogun-mcp` / `shogun-api` exit nonzero with a clear stderr message.
2. **Issue a token** (recommended): Settings → Memory API → Issue. Copy the token once into your MCP client env as `SHOGUN_API_TOKEN`.  
   - If **any** tokens have been issued, stdio MCP **requires** a matching `SHOGUN_API_TOKEN`.  
   - If **none** yet, process-trust is allowed when enabled (dev DX).
3. Soft Pro gate: the Enable toggle is the product gate until Stripe billing (WP5.1). Trial is Pro-equivalent — enabling during trial is allowed.

## Where is `memory.db`?

On macOS the desktop app stores memory under the app data directory, typically:

```text
~/Library/Application Support/com.selectkk.shogun/
  memory.db
  memory_api.json      # enable + profile (display_name, role, prefs)
  l3_approvals.json    # shared L3 queue (MCP actions.execute ↔ Settings → Approvals)
  visual_recall.json
```

(`memory_data_dir` may nest one level; open Settings → Memory API once to create `memory_api.json` next to the live DB.)

Standalone bins use `SHOGUN_DB_PATH` (default `./shogun.db`). Settings resolve as:

1. `SHOGUN_MEMORY_API_SETTINGS` if set, else  
2. `<parent of SHOGUN_DB_PATH>/memory_api.json`

L3 approvals resolve as `SHOGUN_L3_APPROVALS` if set, else `<parent of SHOGUN_DB_PATH>/l3_approvals.json`.

## Build the MCP binary

```bash
cargo build -p shogun-core --features db --bin shogun-mcp --release
# binary: target/release/shogun-mcp
```

## Cursor — `mcp.json`

Place under Cursor’s MCP config (user or project). See also [`docs/examples/cursor-mcp.shogun.json`](examples/cursor-mcp.shogun.json).

```json
{
  "mcpServers": {
    "shogun-memory": {
      "command": "/ABS/PATH/TO/target/release/shogun-mcp",
      "args": [],
      "env": {
        "SHOGUN_DB_PATH": "/Users/YOU/Library/Application Support/com.selectkk.shogun/memory.db",
        "SHOGUN_MEMORY_API_SETTINGS": "/Users/YOU/Library/Application Support/com.selectkk.shogun/memory_api.json",
        "SHOGUN_API_TOKEN": "shogun_PASTE_ISSUED_TOKEN"
      }
    }
  }
}
```

## Claude Desktop

Config file (macOS): `~/Library/Application Support/Claude/claude_desktop_config.json`.  
See [`docs/examples/claude-desktop.shogun.json`](examples/claude-desktop.shogun.json).

```json
{
  "mcpServers": {
    "shogun-memory": {
      "command": "/ABS/PATH/TO/target/release/shogun-mcp",
      "args": [],
      "env": {
        "SHOGUN_DB_PATH": "/Users/YOU/Library/Application Support/com.selectkk.shogun/memory.db",
        "SHOGUN_MEMORY_API_SETTINGS": "/Users/YOU/Library/Application Support/com.selectkk.shogun/memory_api.json",
        "SHOGUN_API_TOKEN": "shogun_PASTE_ISSUED_TOKEN"
      }
    }
  }
}
```

## Tool tips (agents)

| Tool | When |
|------|------|
| `profile.whoami` | Session start: identity + prefs + short work summary (counts/names). Not a search. |
| `memory.get_context` | Compact DB-derived work snapshot (state facts + recent notes). **Live AX Notch cache is not available** to standalone MCP. |
| `memory.search` | Specific keyword / question over events and notes. |
| `memory.append_note` | L1 — append a user note. |
| `profile.set` | L1 — update display_name / role / prefs in `memory_api.json`. |
| `actions.execute` | Run a local action or enqueue an L3 send. Sends return `{pending, approval_id}` and appear in **Settings → Approvals** (shared `l3_approvals.json`). Confirm with the dedicated button — Enter alone never sends. `local_search` runs hybrid search immediately. |

## Smoke check

1. Enable Memory API + issue token in Settings.  
2. Point Cursor mcp.json at `shogun-mcp` with the three env vars.  
3. In Cursor agent chat: call `profile.whoami`, then `memory.get_context`, then `memory.append_note` with a short note.  
4. Disable Memory API in Settings → restart MCP → process should refuse to start (stderr: disabled).
