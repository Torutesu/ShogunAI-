// Shortcut rows, and the recorder behind them.
//
// The chips print the glyph AND the word — `⌃ control`, not `⌃`. A wall of bare glyphs is a
// puzzle: ⌃ and ⌥ are the two most confused symbols on the keyboard, and the person most likely
// to be reading this screen is the one who has just failed to trigger something. The gesture is
// its own quieter chip, because the gesture is what distinguishes two shortcuts that share their
// modifiers, and it must not read as another key.
//
// Recording accepts what the grammar accepts (see shortcuts.ts): modifiers plus a gesture, or a
// modifier chord plus a key. `fn` cannot be observed from a webview — WebKit does not report it —
// so it is armed with a control instead of pressed, which is honest and keeps it in the
// vocabulary rather than reserving it for defaults nobody can reproduce.

import { useCallback, useEffect, useRef, useState } from "react";
import type { JSX } from "react";
import { invoke } from "@tauri-apps/api/core";
import { IN_TAURI } from "./tauri";
import { t } from "./strings";
import {
  ACTIONS,
  DEFAULT_TRIGGERS,
  MOD_LABEL,
  formatTrigger,
  keyLabel,
  parseTrigger,
  shadows,
} from "./shortcuts";
import type { Action, Gesture, Mod, Trigger } from "./shortcuts";

/** Held longer than this and it was a hold, not a tap. */
const HOLD_MS = 350;
/** A second tap inside this window makes it a double. */
const DOUBLE_MS = 420;

const GESTURE_LABEL: Record<Gesture, string> = {
  hold: t.gestureHold,
  tap: t.gestureTap,
  double: t.gestureDouble,
  press: "",
};

/** One trigger, rendered. Shared with onboarding's closing step, which teaches these same two. */
export function TriggerChips(props: { trigger: Trigger }): JSX.Element {
  const { trigger } = props;
  return (
    <span className="trig">
      {trigger.mods.map((m) => (
        <kbd key={m} className="trig__key">
          {MOD_LABEL[m].glyph ? <i className="trig__glyph">{MOD_LABEL[m].glyph}</i> : null}
          {MOD_LABEL[m].name}
        </kbd>
      ))}
      {trigger.key ? <kbd className="trig__key">{keyLabel(trigger.key)}</kbd> : null}
      {/* Always rendered, empty for a key chord: it reserves the column so every row's keys end
          at the same edge instead of stepping in and out by the width of "2×". */}
      <span className="trig__gesture">{GESTURE_LABEL[trigger.gesture]}</span>
    </span>
  );
}

/**
 * Watch the keyboard and work out which gesture was performed.
 *
 * A tap can only be recognised on the way UP, and a double only after the tap window closes, so
 * this reports through a callback rather than returning — and the pending tap is cancelled the
 * moment a real key joins in, which is what makes `Alt:tap` and `Control+Alt+KeyQ` coexist.
 */
function useRecorder(active: boolean, fnArmed: boolean, onCapture: (t: Trigger) => void): void {
  const down = useRef<{ mods: Mod[]; at: number } | null>(null);
  const pendingTap = useRef<{ mods: Mod[]; timer: number } | null>(null);
  const captured = useRef(false);

  useEffect(() => {
    if (!active) return;
    captured.current = false;

    const modsOf = (e: KeyboardEvent): Mod[] => {
      const m: Mod[] = [];
      if (fnArmed) m.push("Function");
      if (e.ctrlKey) m.push("Control");
      if (e.altKey) m.push("Alt");
      if (e.shiftKey) m.push("Shift");
      if (e.metaKey) m.push("Super");
      return m;
    };
    const emit = (trigger: Trigger): void => {
      if (captured.current) return;
      captured.current = true;
      onCapture(trigger);
    };
    const clearPending = (): void => {
      if (pendingTap.current) {
        clearTimeout(pendingTap.current.timer);
        pendingTap.current = null;
      }
    };

    const onDown = (e: KeyboardEvent): void => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") return;
      const mods = modsOf(e);
      const isMod = ["Control", "Alt", "Shift", "Meta"].includes(e.key);
      if (isMod) {
        if (!down.current) down.current = { mods, at: performance.now() };
        else down.current.mods = mods; // a second modifier joined the set
        return;
      }
      // A real key ends it immediately: this is a chord, not a gesture.
      clearPending();
      if (mods.length > 0) emit({ mods, gesture: "press", key: e.code });
    };

    const onUp = (e: KeyboardEvent): void => {
      e.preventDefault();
      const start = down.current;
      if (!start) return;
      // Only decide once the LAST modifier comes up, or ⌃⌥ would resolve on the ⌃ release.
      if (e.ctrlKey || e.altKey || e.shiftKey || e.metaKey) return;
      down.current = null;
      const held = performance.now() - start.at;
      const mods = start.mods;
      if (held >= HOLD_MS) {
        clearPending();
        emit({ mods, gesture: "hold" });
        return;
      }
      const key = mods.join("+");
      if (pendingTap.current && pendingTap.current.mods.join("+") === key) {
        clearPending();
        emit({ mods, gesture: "double" });
        return;
      }
      clearPending();
      const timer = window.setTimeout(() => {
        pendingTap.current = null;
        emit({ mods, gesture: "tap" });
      }, DOUBLE_MS);
      pendingTap.current = { mods, timer };
    };

    // Capture phase at the window: relying on a button's own focus proved fragile on device —
    // clicks landed but keystrokes never did, and rebinding looked broken.
    window.addEventListener("keydown", onDown, true);
    window.addEventListener("keyup", onUp, true);
    return () => {
      window.removeEventListener("keydown", onDown, true);
      window.removeEventListener("keyup", onUp, true);
      clearPending();
      down.current = null;
    };
  }, [active, fnArmed, onCapture]);
}

export function ShortcutRows(): JSX.Element {
  const [binds, setBinds] = useState<Record<string, string>>(DEFAULT_TRIGGERS);
  const [recording, setRecording] = useState<Action | null>(null);
  const [fnArmed, setFnArmed] = useState(false);
  const [error, setError] = useState("");

  const refresh = useCallback((): void => {
    if (!IN_TAURI) return;
    void invoke<Record<string, string>>("get_shortcuts")
      .then((b) => setBinds({ ...DEFAULT_TRIGGERS, ...b }))
      .catch(() => undefined);
  }, []);
  useEffect(refresh, [refresh]);

  const save = useCallback(
    (action: Action, trigger: Trigger): void => {
      const combo = formatTrigger(trigger);
      setRecording(null);
      setFnArmed(false);
      setBinds((b) => ({ ...b, [action]: combo }));
      if (!IN_TAURI) return;
      void invoke("set_shortcut", { action, combo })
        .then(refresh)
        .catch((e) => setError(String(e)));
    },
    [refresh],
  );

  const onCapture = useCallback(
    (trigger: Trigger): void => {
      if (recording) save(recording, trigger);
    },
    [recording, save],
  );
  useRecorder(recording !== null, fnArmed, onCapture);

  // Escape leaves recording without binding anything.
  useEffect(() => {
    if (!recording) return;
    const onEsc = (e: KeyboardEvent): void => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      setRecording(null);
      setFnArmed(false);
    };
    window.addEventListener("keydown", onEsc, true);
    return () => window.removeEventListener("keydown", onEsc, true);
  }, [recording]);

  const triggers = ACTIONS.map((a) => ({
    action: a,
    trigger: parseTrigger(binds[a] ?? DEFAULT_TRIGGERS[a]),
  }));

  /// A tap pre-empts every slower gesture on the same modifiers, so the collision worth warning
  /// about is not just "identical" — see `shadows`.
  const shadowed = new Set<string>();
  for (const a of triggers) {
    for (const b of triggers) {
      if (a.action === b.action || !a.trigger || !b.trigger) continue;
      if (shadows(a.trigger, b.trigger)) shadowed.add(b.action);
    }
  }

  const isDefault = ACTIONS.every((a) => (binds[a] ?? DEFAULT_TRIGGERS[a]) === DEFAULT_TRIGGERS[a]);

  return (
    <>
      {triggers.map(({ action, trigger }) => (
        <div key={action} className="keys">
          <span className="keys__name">{t.actionLabel[action]}</span>
          {recording === action ? (
            <span className="rec">
              <button
                type="button"
                className={`rec__fn${fnArmed ? " is-on" : ""}`}
                title={t.recordFnHint}
                onClick={() => setFnArmed((v) => !v)}
              >
                fn
              </button>
              <button className="keys__rec" type="button" onClick={() => setRecording(null)}>
                {t.recordHint}
              </button>
            </span>
          ) : (
            <button
              className={`keys__btn${shadowed.has(action) ? " is-warn" : ""}`}
              type="button"
              title={t.change}
              onClick={() => {
                setRecording(action);
                setFnArmed(false);
                setError("");
              }}
            >
              {trigger ? <TriggerChips trigger={trigger} /> : null}
            </button>
          )}
        </div>
      ))}
      {shadowed.size > 0 ? <div className="set__hint is-warn">{t.shortcutShadowed}</div> : null}
      {error ? <div className="set__hint is-err">{error}</div> : null}
      <div className="set__hint">{t.shortcutHint}</div>
      {!isDefault ? (
        <div className="keyrow">
          <button
            className="keyrow__btn"
            type="button"
            onClick={() => ACTIONS.forEach((a) => save(a, parseTrigger(DEFAULT_TRIGGERS[a])!))}
          >
            {t.shortcutReset}
          </button>
        </div>
      ) : null}
    </>
  );
}
