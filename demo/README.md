# ShogunAI demo — recording prop

`shogunai-demo.html` is a standalone page for screen-recording a product demo.
It is deliberately **not** part of `apps/website`: nothing serves it, nothing
links to it, and it ships with no build step. Open the file in a browser.

```
open demo/shogunai-demo.html
```

It has no dependencies, makes no network requests, and loads no external font
or image — everything is inline CSS, inline SVG and a little vanilla JS. That
also means it works from `file://`.

## Recording it

| Key | Does |
|---|---|
| `Space` / `→` | next beat |
| `←` | previous beat |
| `A` | auto-play (3.8s per beat) |
| `R` | back to the start |
| `H` | hide the on-screen controls — press this before you hit record |

The clock is pinned to 9:41 so separate takes match.

## The script

Seven beats, all preset, all mock data:

1. Idle desktop, panel hanging from the notch
2. "What is still open before tomorrow's v1.0?" types itself in
3. Three open loops come back, each with its source and time
4. The L1 tidies itself and closes its loop
5. Option types a release note into a mail window, at the caret
6. The L3 send stops at the approval gate; approving prints the traceability rows
7. A meeting runs: live transcript, then minutes with proposed next actions

## Changing it

Everything is in one file. The script lives in the `steps` array near the
bottom — each entry is `{ run(animate) }`, where `animate` is false when the
beat is being replayed rather than played, so it should snap to its finished
state instead of re-typing. Keep that contract or stepping backwards leaves
half-typed sentences on screen.
