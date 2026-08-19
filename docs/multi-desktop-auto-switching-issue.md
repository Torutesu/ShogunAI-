# Issue: Make the desktop experience responsive across multiple desktops

**Suggested labels:** `desktop`, `ux`, `multi-monitor`, `P1`

## Problem

On multi-monitor and multi-Space setups, the Shogun notch/panel can feel slow or appear to stay associated with the wrong desktop/display. Switching between desktops should not require manually recovering the panel or hunting for where it moved.

## Goal

Make display and Space changes feel automatic and immediate while keeping a simple manual override available for users who want to pin Shogun to a specific display.

## Scope

- Detect active Space/display changes and converge the panel to the appropriate display automatically.
- Prefer the display containing the active Space/window context, with a deterministic fallback when that context is unavailable.
- Keep transitions responsive during rapid Space swipes, monitor attach/detach, lid close/open, fullscreen changes, and display arrangement changes.
- Avoid stale reposition requests racing newer display decisions.
- Provide an obvious manual display switcher with the current display marked.
- Persist the user’s display preference without preventing automatic recovery when that display is unavailable.
- Keep the notch/panel visible or recover it quickly after a display change; do not leave an invisible or click-through orphan window.

## Acceptance criteria

- Switching Spaces repeatedly does not leave the panel on the previous Space or require restarting Shogun.
- Connecting, disconnecting, or rearranging displays rehomes the panel automatically within 2 seconds.
- Clamshell open/close and fullscreen transitions recover the panel without a duplicate window or stale position.
- Manual display selection takes effect immediately and remains the preference until the selected display is unavailable.
- When the preferred display disappears, Shogun falls back deterministically and returns to the preference when it becomes available again.
- Rapid consecutive display-change events settle on the latest valid display decision; stale requests cannot move the panel afterward.
- Add observable display-change/recovery diagnostics sufficient to distinguish detection, selection, reposition, and recovery failures.

## Out of scope

- Redesigning the notch or expanded-panel visual language.
- Changing voice, meeting, or transcript behavior.
- Supporting non-macOS window-management semantics in this issue.
