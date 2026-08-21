# Design QA — Memory layer simplification

## Evidence

- Source visual: `/var/folders/73/8h5shzqn3nj4zmn32ntdtp6c0000gn/T/TemporaryItems/NSIRD_screencaptureui_d01vrJ/スクリーンショット 2026-08-21 12.09.01.png`
- Source size: 2890 × 1388
- Implementation capture: `/tmp/shogun-memory-desktop-final.png`
- Implementation size: 1280 × 720, device scale factor 1
- Side-by-side comparison: `/tmp/shogun-memory-qa-comparison.png`
- State: Japanese locale, light theme, `#memory`

## Intentional change

The source used a long implementation explanation, three sentence-length bullets, and a search-results card. The implementation reduces the message to one outcome and replaces the search card with a visible three-step product flow: work is seen across real apps, saved as a sourced memory, and recalled through a plain-language question.

## Visual review

- P0: none
- P1: none
- P2: none
- P3: none blocking handoff

The simplified section preserves the existing page grid, spacing, typography, color tokens, border radii, and card treatment. Gmail, Notion, and Google Calendar use the repository's real brand assets. The information hierarchy is now readable without relying on the supporting paragraph.

## Responsive and state checks

- Desktop 1280: no document overflow; memory flow card is 576 px wide and 547 px tall.
- EN, JA, ES, and DE at desktop width: identical card bounds and no horizontal overflow.
- Dark mode: section background, cards, borders, labels, and brand marks retain readable contrast.
- The single-column mobile breakpoint and compact three-source grid were reviewed in the implementation; the selected in-app browser did not expose viewport resizing for a separate mobile capture in this run.
- Interaction: the memory CTA navigates to the localized `/features/ai-memory` page.

## Iteration history

1. Replaced the text-heavy result card with a three-step memory flow.
2. Reduced the body and bullets to short outcome-oriented copy in all four locales.
3. Shortened the Japanese headline to avoid an awkward three-line wrap.
4. Verified real source logos, light and dark themes, localized layouts, and the detail CTA.

final result: passed
