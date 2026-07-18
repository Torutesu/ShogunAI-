# ShogunAI — Landing Page & UI

Marketing landing page and product UI shell for **ShogunAI** — the AI that
remembers your day and acts on it.

## Design

The visual language is adapted from the **Aside Skyglass** design system:
bright, airy, and optimistic, with a sky-and-cloud backdrop, crisp typography,
and restrained rounded (pill) controls.

- **Colors** — near-black `#090B0C` for text/controls, sky-blue accent `#00A6F4`
  for links and emphasis, white surfaces, and soft sky tints for atmosphere.
- **Typography** — Space Grotesk for display headlines, Geist for UI/body copy.
- **Shape** — full-radius pills for buttons, inputs, and chips; `8px` cards.
- **Depth** — layering and 1px borders over heavy shadows.

Tokens live in [`assets/tokens.css`](assets/tokens.css); page layout in
[`assets/page.css`](assets/page.css).

## Structure

| File | Purpose |
| --- | --- |
| `index.html` | Landing page (hero, memory, action, how-it-works, pricing, CTA) |
| `assets/tokens.css` | Design tokens: colors, type scale, buttons, chips, cards |
| `assets/page.css` | Page-specific layout and responsive rules |

## Run locally

It's a static site — open `index.html` directly, or serve it:

```bash
python3 -m http.server 8000
# then open http://localhost:8000
```
