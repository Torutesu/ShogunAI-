import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// The product window (M1 shell start). A normal, visible, decorated window — not the fragile notch
// overlay. It drives the real backend: `inline_at_cursor` (⌃⌥G draft-at-cursor) and `notch_actions`
// (context actions from memory). When opened in a plain browser (no Tauri) it renders with mock data
// so the design can be iterated without a Mac.

const IN_TAURI = typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

interface ActionView {
  label: string;
  level: "L1" | "L2" | "L3";
  rationale: string;
}

const MOCK_ACTIONS: ActionView[] = [
  { label: "Draft reply", level: "L1", rationale: "reply-needed loop · Q3 roadmap thread" },
  { label: "Nudge: legal sign-off", level: "L2", rationale: "waiting on legal to reply" },
  { label: "Search memory", level: "L1", rationale: "Search memory" },
];

type Appearance = "auto" | "light" | "dark";

export function App(): JSX.Element {
  const [actions, setActions] = useState<ActionView[]>(IN_TAURI ? [] : MOCK_ACTIONS);
  const [draftStatus, setDraftStatus] = useState<string>("");
  const [appearance, setAppearance] = useState<Appearance>("dark");
  const busy = useRef(false);

  // Apply the theme to <html> (dark is the default look; light/auto set the attribute).
  useEffect(() => {
    const el = document.documentElement;
    if (appearance === "dark") el.removeAttribute("data-appearance");
    else el.setAttribute("data-appearance", appearance);
  }, [appearance]);

  // Pull the current context actions from memory, and refresh periodically.
  useEffect(() => {
    if (!IN_TAURI) return;
    let live = true;
    const pull = (): void => {
      void invoke<ActionView[]>("notch_actions")
        .then((a) => {
          if (live && Array.isArray(a)) setActions(a);
        })
        .catch(() => {
          /* backend not ready — keep what we have */
        });
    };
    pull();
    const id = setInterval(pull, 3000);
    return () => {
      live = false;
      clearInterval(id);
    };
  }, []);

  const draftAtCursor = (): void => {
    if (busy.current) return;
    busy.current = true;
    setDraftStatus("Working — put your cursor in the app you want to write in…");
    if (!IN_TAURI) {
      setTimeout(() => {
        setDraftStatus("Inserted at your cursor (preview).");
        busy.current = false;
      }, 900);
      return;
    }
    void invoke<string>("inline_at_cursor")
      .then(() => setDraftStatus("Started — the draft appears where your cursor is. (⌘Z to undo.)"))
      .catch((e) => setDraftStatus(`Couldn't start: ${e}`))
      .finally(() => {
        busy.current = false;
      });
  };

  const runAction = (index: number): void => {
    if (!IN_TAURI) return;
    void invoke<string>("run_notch_action", { index }).catch(() => undefined);
  };

  return (
    <div className="app">
      <header className="bar">
        <div className="brand">
          <span className="brand__mark">⚔</span>
          <span className="brand__name">SHOGUN</span>
          <span className="live" title="Reading the screen">
            <span className="live__dot" /> reading
          </span>
        </div>
        <div className="seg" role="group" aria-label="Appearance">
          {(["auto", "light", "dark"] as Appearance[]).map((a) => (
            <button key={a} type="button" aria-pressed={appearance === a} onClick={() => setAppearance(a)}>
              {a[0].toUpperCase() + a.slice(1)}
            </button>
          ))}
        </div>
      </header>

      <main className="main">
        <section className="hero">
          <h1 className="hero__title">Write anywhere, from your cursor.</h1>
          <p className="hero__sub">
            Put your cursor in any app — an email, a message, a doc — and SHOGUN writes the best
            continuation right there, from what's on your screen and what it remembers.
          </p>
          <div className="cta">
            <button className="btn btn--primary" type="button" onClick={draftAtCursor}>
              Draft at cursor
            </button>
            <span className="cta__hint">
              or press <kbd>⌃</kbd> <kbd>⌥</kbd> <kbd>G</kbd> anywhere
            </span>
          </div>
          {draftStatus ? <div className="status">{draftStatus}</div> : null}
        </section>

        <section className="panel">
          <div className="panel__h">Suggested from your memory</div>
          {actions.length > 0 ? (
            <div className="rows">
              {actions.map((a, i) => (
                <button key={i} className="row" type="button" title={a.rationale} onClick={() => runAction(i)}>
                  <span className="row__label">{a.label}</span>
                  <span className="row__why">{a.rationale}</span>
                </button>
              ))}
            </div>
          ) : (
            <div className="empty">Nothing pressing right now. Keep working — SHOGUN is watching for what matters.</div>
          )}
        </section>
      </main>

      <footer className="foot">
        <span>Drafts stay on this Mac. SHOGUN never sends — you do, in your own app.</span>
      </footer>
    </div>
  );
}
