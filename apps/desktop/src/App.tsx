import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// SHOGUN product window. Real, live, wired to the backend:
//  - shogun_status : what SHOGUN is reading + how much it knows (polled)
//  - shogun_state  : the commitments / open loops it tracks (polled)
//  - shogun_chat   : ask it anything, grounded in memory, on the BYOK Agent lane
//  - inline_at_cursor : the ⌃⌥G draft-at-cursor
// In a plain browser (no Tauri) it runs on mock data so the design iterates without a Mac.

const IN_TAURI = typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

interface Status {
  app: string;
  commitments: number;
  open_loops: number;
  has_key: boolean;
}
interface StateItem {
  text: string;
  meta: string;
}
interface StateView {
  commitments: StateItem[];
  open_loops: StateItem[];
}
type Msg = { role: "me" | "shogun"; text: string };
type Appearance = "auto" | "light" | "dark";

const MOCK_STATUS: Status = { app: "com.apple.mail", commitments: 2, open_loops: 1, has_key: false };
const MOCK_STATE: StateView = {
  commitments: [
    { text: "Send Alice the Q3 deck", meta: "overdue" },
    { text: "Reply to the vendor about pricing", meta: "70% sure" },
  ],
  open_loops: [{ text: "Waiting on legal sign-off", meta: "3d waiting" }],
};

/** A friendly app name from a bundle id ("com.apple.mail" → "Mail"). */
function appName(bundle: string): string {
  if (!bundle) return "your screen";
  const seg = bundle.split(".").pop() || bundle;
  return seg.charAt(0).toUpperCase() + seg.slice(1);
}

export function App(): JSX.Element {
  const [status, setStatus] = useState<Status | null>(IN_TAURI ? null : MOCK_STATUS);
  const [state, setState] = useState<StateView>(IN_TAURI ? { commitments: [], open_loops: [] } : MOCK_STATE);
  const [msgs, setMsgs] = useState<Msg[]>([]);
  const [input, setInput] = useState("");
  const [thinking, setThinking] = useState(false);
  const [appearance, setAppearance] = useState<Appearance>("dark");
  const [draftNote, setDraftNote] = useState("");
  const threadRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = document.documentElement;
    if (appearance === "dark") el.removeAttribute("data-appearance");
    else el.setAttribute("data-appearance", appearance);
  }, [appearance]);

  // live polling
  useEffect(() => {
    if (!IN_TAURI) return;
    let live = true;
    const pull = (): void => {
      void invoke<Status>("shogun_status").then((s) => live && setStatus(s)).catch(() => undefined);
      void invoke<StateView>("shogun_state").then((s) => live && s && setState(s)).catch(() => undefined);
    };
    pull();
    const id = setInterval(pull, 3000);
    return () => {
      live = false;
      clearInterval(id);
    };
  }, []);

  useEffect(() => {
    threadRef.current?.scrollTo({ top: threadRef.current.scrollHeight, behavior: "smooth" });
  }, [msgs, thinking]);

  const send = (): void => {
    const q = input.trim();
    if (!q || thinking) return;
    setInput("");
    setMsgs((m) => [...m, { role: "me", text: q }]);
    setThinking(true);
    const finish = (text: string): void => {
      setMsgs((m) => [...m, { role: "shogun", text }]);
      setThinking(false);
    };
    if (!IN_TAURI) {
      setTimeout(() => finish("Here's what I'd do, grounded in your open loops — but connect a key to get real answers."), 900);
      return;
    }
    void invoke<string>("shogun_chat", { message: q })
      .then((r) => finish(r || "(no response)"))
      .catch((e) => finish(`Couldn't answer: ${e}`));
  };

  const draftAtCursor = (): void => {
    setDraftNote("Put your cursor in any app, then press ⌃⌥G — the draft appears right there.");
    if (IN_TAURI) void invoke("inline_at_cursor").catch(() => undefined);
  };

  const live = appName(status?.app || "");

  return (
    <div className="app">
      <header className="bar">
        <div className="brand">
          <span className="brand__mark">⚔</span>
          <span className="brand__name">SHOGUN</span>
          <span className="live" title="SHOGUN is reading the focused app">
            <span className="live__dot" /> reading {live}
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

      <div className="body">
        <main className="chat">
          <div className="thread" ref={threadRef}>
            {msgs.length === 0 ? (
              <div className="welcome">
                <h1>What can I take off your plate?</h1>
                <p>
                  Ask me anything about your work — I answer from what's on your screen and what I remember. Or put
                  your cursor in any app and press <kbd>⌃</kbd><kbd>⌥</kbd><kbd>G</kbd> to draft right there.
                </p>
                {!status?.has_key && IN_TAURI ? (
                  <p className="hintkey">No key yet — I'll echo for now. Add your key to get real answers.</p>
                ) : null}
              </div>
            ) : (
              msgs.map((m, i) => (
                <div key={i} className={`msg msg--${m.role}`}>
                  {m.text}
                </div>
              ))
            )}
            {thinking ? <div className="msg msg--shogun msg--thinking">…</div> : null}
          </div>

          {draftNote ? <div className="draftnote">{draftNote}</div> : null}

          <div className="composer">
            <button className="composer__draft" type="button" onClick={draftAtCursor} title="Draft at your cursor (⌃⌥G)">
              ✎ Draft at cursor
            </button>
            <input
              className="composer__input"
              placeholder="Ask SHOGUN, or tell it what to do…"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  send();
                }
              }}
            />
            <button className="composer__send" type="button" onClick={send} disabled={!input.trim() || thinking}>
              ↑
            </button>
          </div>
        </main>

        <aside className="side">
          <div className="side__group">
            <div className="side__h">Commitments</div>
            {state.commitments.length ? (
              state.commitments.map((c, i) => (
                <div key={i} className="item">
                  <div className="item__text">{c.text}</div>
                  <div className={`item__meta ${c.meta === "overdue" ? "is-over" : ""}`}>{c.meta}</div>
                </div>
              ))
            ) : (
              <div className="item item--empty">Nothing due.</div>
            )}
          </div>
          <div className="side__group">
            <div className="side__h">Open loops</div>
            {state.open_loops.length ? (
              state.open_loops.map((l, i) => (
                <div key={i} className="item">
                  <div className="item__text">{l.text}</div>
                  <div className="item__meta">{l.meta}</div>
                </div>
              ))
            ) : (
              <div className="item item--empty">Nothing waiting.</div>
            )}
          </div>
        </aside>
      </div>

      <footer className="foot">Everything stays on this Mac. SHOGUN drafts — you send, in your own app.</footer>
    </div>
  );
}
