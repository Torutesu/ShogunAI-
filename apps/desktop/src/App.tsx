import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// SHOGUN notch panel — a translucent glass panel that hangs from the notch. Chat-first, live,
// wired to the backend (shogun_status / shogun_state / shogun_chat / inline_at_cursor). In a plain
// browser (no Tauri) it runs on mock data so the design iterates without a Mac.

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
  const [showState, setShowState] = useState(false);
  const threadRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = document.documentElement;
    if (appearance === "dark") el.removeAttribute("data-appearance");
    else el.setAttribute("data-appearance", appearance);
  }, [appearance]);

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
      setTimeout(() => finish("I'd start with the overdue deck for Alice — want me to draft it at your cursor?"), 800);
      return;
    }
    void invoke<string>("shogun_chat", { message: q })
      .then((r) => finish(r || "(no response)"))
      .catch((e) => finish(`Couldn't answer: ${e}`));
  };

  const draftAtCursor = (): void => {
    if (IN_TAURI) void invoke("inline_at_cursor").catch(() => undefined);
  };

  const totalState = state.commitments.length + state.open_loops.length;
  const live = appName(status?.app || "");

  return (
    <div className="wrap">
      <div className="tongue" />
      <div className="panel">
        <header className="head">
          <span className="live">
            <span className="live__dot" />
            reading <b>{live}</b>
          </span>
          <div className="head__right">
            {totalState > 0 ? (
              <button className="chip" type="button" onClick={() => setShowState((v) => !v)} aria-pressed={showState}>
                {state.commitments.length} due · {state.open_loops.length} waiting
              </button>
            ) : null}
            <button
              className="icon"
              type="button"
              title="Theme"
              onClick={() => setAppearance((a) => (a === "dark" ? "light" : a === "light" ? "auto" : "dark"))}
            >
              {appearance === "dark" ? "◐" : appearance === "light" ? "◑" : "◒"}
            </button>
          </div>
        </header>

        {showState ? (
          <div className="state">
            {state.commitments.map((c, i) => (
              <div key={`c${i}`} className="state__row">
                <span className="state__text">{c.text}</span>
                <span className={`state__meta ${c.meta === "overdue" ? "is-over" : ""}`}>{c.meta}</span>
              </div>
            ))}
            {state.open_loops.map((l, i) => (
              <div key={`l${i}`} className="state__row">
                <span className="state__text">{l.text}</span>
                <span className="state__meta">{l.meta}</span>
              </div>
            ))}
          </div>
        ) : null}

        <div className="thread" ref={threadRef}>
          {msgs.length === 0 ? (
            <div className="welcome">
              <div className="welcome__t">What can I take off your plate?</div>
              <div className="welcome__s">
                Ask about your work, or press <kbd>⌃</kbd><kbd>⌥</kbd><kbd>G</kbd> in any app to draft at your cursor.
              </div>
              {IN_TAURI && status && !status.has_key ? (
                <div className="welcome__key">No key yet — I'll echo. Add a key for real answers.</div>
              ) : null}
            </div>
          ) : (
            msgs.map((m, i) => (
              <div key={i} className={`msg msg--${m.role}`}>
                {m.text}
              </div>
            ))
          )}
          {thinking ? <div className="msg msg--shogun msg--think">…</div> : null}
        </div>

        <div className="composer">
          <button className="composer__draft" type="button" onClick={draftAtCursor} title="Draft at your cursor (⌃⌥G)">
            ✎
          </button>
          <input
            className="composer__input"
            placeholder="Ask SHOGUN…"
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
      </div>
    </div>
  );
}
