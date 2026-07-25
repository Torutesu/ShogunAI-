import { useCallback, useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { App } from "../App";
import { DESK_H, DESK_W, NOTCH_H, NOTCH_W, INITIAL, emit, store } from "./bridge";
import type { ConnState, Scenario } from "./bridge";

// Browser preview stage. The panel is the REAL App component; everything around it is a stand-in
// for the Mac it normally hangs off — wallpaper, menu bar, notch, and a window underneath so the
// glass has something to blur. The desk is laid out in true macOS points (1512×944, a 14" MacBook
// Pro) and scaled to fit, so what you see is the panel at its real size, not an impression of it.
//
// The rail on the right is developer tooling, not product UI. It exists so every state the panel
// can be in — a rejected key, a service that needs reauth, a nightly run that failed, an approval
// waiting to be sent — can be reached in a second instead of being reproduced on device.

function useScenario(): Scenario {
  return useSyncExternalStore(store.subscribe, store.get, store.get);
}

interface DeskApp {
  bundle: string;
  name: string;
  menus: string[];
}

const APPS: DeskApp[] = [
  { bundle: "com.apple.mail", name: "Mail", menus: ["File", "Edit", "View", "Mailbox", "Message"] },
  { bundle: "com.tinyspeck.slackmacgap", name: "Slack", menus: ["File", "Edit", "View", "Window"] },
  { bundle: "com.figma.Desktop", name: "Figma", menus: ["File", "Edit", "View", "Object"] },
  { bundle: "com.microsoft.VSCode", name: "Code", menus: ["File", "Edit", "Selection", "Go"] },
  { bundle: "com.apple.Safari", name: "Safari", menus: ["File", "Edit", "View", "History"] },
];

type Wallpaper = "dusk" | "graphite" | "linen";
type DeskMode = "notch" | "pseudo";

const WALLPAPERS: Array<{ id: Wallpaper; label: string }> = [
  { id: "dusk", label: "Dusk" },
  { id: "graphite", label: "Graphite" },
  { id: "linen", label: "Linen" },
];

const CONN_STATES: ConnState[] = ["connected", "needs_reauth", "disconnected", "coming_soon"];

export function Stage(): JSX.Element {
  const s = useScenario();
  const [wallpaper, setWallpaper] = useState<Wallpaper>("dusk");
  const [mode, setMode] = useState<DeskMode>("notch");
  const [zoom, setZoom] = useState<number>(0);   // 0 = fit to viewport
  const [fit, setFit] = useState(1);
  const [railOpen, setRailOpen] = useState(true);
  const [grid, setGrid] = useState(false);

  const app = APPS.find((a) => a.bundle === s.foreground) ?? APPS[0];

  // Fit the desk to whatever room the viewport gives us, so the panel is never cropped.
  useEffect(() => {
    const measure = (): void => {
      const railW = railOpen ? 340 : 0;
      const w = (window.innerWidth - railW - 64) / DESK_W;
      const h = (window.innerHeight - 64) / DESK_H;
      setFit(Math.min(w, h, 1));
    };
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [railOpen]);

  const scale = zoom || fit;

  // The foreground app is a Rust-side `context` event on device — push it the same way here.
  useEffect(() => {
    emit("context", {
      bundle_id: s.foreground,
      title_masked: app.name,
      text: "",
      captured_at_ms: Date.now(),
      partial: false,
    });
  }, [s.foreground, app.name]);

  const summon = useCallback((): void => emit("summon", {}), []);

  // The panel's own theme. It is App state (persisted, and settable from Settings), so the rail
  // drives it the way the App itself does — stamp the attribute, persist the choice — and watches
  // the attribute so changing it inside Settings keeps the rail honest.
  const [appearance, setAppearance] = useState<string>(
    () => document.documentElement.getAttribute("data-appearance") ?? "dark",
  );
  useEffect(() => {
    const ob = new MutationObserver(() =>
      setAppearance(document.documentElement.getAttribute("data-appearance") ?? "auto"),
    );
    ob.observe(document.documentElement, { attributes: true, attributeFilter: ["data-appearance"] });
    return () => ob.disconnect();
  }, []);
  const applyAppearance = useCallback((next: string): void => {
    localStorage.setItem("shogun.appearance", JSON.stringify(next));
    document.documentElement.setAttribute("data-appearance", next);
    setAppearance(next);
  }, []);

  const reset = useCallback((): void => {
    // The App persists view state across the Rust-driven window respawns; a reset has to clear
    // that too or "back to the default state" silently keeps the last session's size.
    Object.keys(localStorage)
      .filter((k) => k.startsWith("shogun."))
      .forEach((k) => localStorage.removeItem(k));
    location.reload();
  }, []);

  const panelLeft = useMemo(() => {
    // Keep the panel on the desk even when it has been dragged wide by the corner grip.
    return Math.max(0, Math.min(s.panel.left, DESK_W - s.panel.w));
  }, [s.panel.left, s.panel.w]);

  return (
    <div className="pv" data-wall={wallpaper}>
      <div className="pv__stage">
        <div
          className="pv__desk"
          data-mode={mode}
          style={{ width: DESK_W, height: DESK_H, transform: `scale(${scale})` }}
        >
          <DeskBackdrop app={app} />

          <div className="pv__menubar">
            <div className="pv__menubar-left">
              <span className="pv__apple" aria-hidden="true">
                ⚔
              </span>
              <b>{app.name}</b>
              {app.menus.map((m) => (
                <span key={m}>{m}</span>
              ))}
            </div>
            <div className="pv__menubar-right">
              <span>100%</span>
              <span aria-hidden="true">◑</span>
              <span aria-hidden="true">⌁</span>
              <span>Fri 25 Jul</span>
              <span>9:41</span>
            </div>
          </div>

          {mode === "notch" ? (
            <div className="pv__notch" style={{ width: NOTCH_W, height: NOTCH_H }} aria-hidden="true">
              <span className="pv__camera" />
            </div>
          ) : null}

          {grid ? <div className="pv__grid" aria-hidden="true" /> : null}

          {/* The panel host is sized ONLY by the App's own set_panel_size calls (see bridge.ts),
              so collapse, settings and the corner grip move real geometry, not a preview guess. */}
          <div
            className="pv__panelhost"
            style={{ top: mode === "notch" ? NOTCH_H : 26, left: panelLeft, width: s.panel.w, height: s.panel.h }}
          >
            {/* The App reads onboarding state once, on mount — the way it does on device, where
                nothing changes it behind the app's back. The rail DOES change it behind its back,
                so the key remounts the App when it moves. */}
            <App key={`ob:${s.onboarding.completed}:${s.onboarding.step}`} />
          </div>
        </div>

        <div className="pv__stagefoot">
          <span>
            desk {DESK_W}×{DESK_H}pt · panel {Math.round(s.panel.w)}×{Math.round(s.panel.h)}pt ·{" "}
            {Math.round(scale * 100)}%
          </span>
        </div>
      </div>

      <button
        className="pv__railtoggle"
        type="button"
        onClick={() => setRailOpen((v) => !v)}
        title={railOpen ? "Hide controls" : "Show controls"}
      >
        {railOpen ? "›" : "‹"}
      </button>

      {railOpen ? (
        <aside className="pv__rail">
          <header className="pv__railhead">
            <span className="pv__railtitle">SHOGUN — panel preview</span>
            <span className="pv__railsub">
              Browser stand-in for the Mac. The panel is the real component; everything else here is
              scaffolding.
            </span>
          </header>

          <Group title="Stage">
            <Seg
              value={mode}
              options={[
                { id: "notch", label: "Notch" },
                { id: "pseudo", label: "No notch" },
              ]}
              onChange={(v) => setMode(v as DeskMode)}
            />
            <Seg
              value={String(zoom)}
              options={[
                { id: "0", label: "Fit" },
                { id: "0.75", label: "75%" },
                { id: "1", label: "100%" },
              ]}
              onChange={(v) => setZoom(Number(v))}
            />
            <Seg
              value={wallpaper}
              options={WALLPAPERS.map((w) => ({ id: w.id, label: w.label }))}
              onChange={(v) => setWallpaper(v as Wallpaper)}
            />
            <Seg
              value={appearance}
              options={[
                { id: "dark", label: "Dark" },
                { id: "light", label: "Light" },
                { id: "auto", label: "Auto" },
              ]}
              onChange={applyAppearance}
            />
            <Row>
              <Toggle label="Layout grid" on={grid} onChange={setGrid} />
            </Row>
            <Row>
              <button className="pv__btn" type="button" onClick={summon}>
                Summon (⌃⌥N)
              </button>
              <button className="pv__btn" type="button" onClick={reset}>
                Reset panel
              </button>
            </Row>
            <p className="pv__hint">
              Open Settings with the ⚙ in the panel. Minimize (▁) collapses it to the notch pill.
              {scale < 1
                ? " Below 100% the corner grip drags in scaled pixels — resize at 100% to judge real sizes."
                : ""}
            </p>
          </Group>

          <Group title="First run">
            <Row>
              <button
                className="pv__btn"
                type="button"
                onClick={() =>
                  store.set((cur) => ({
                    onboarding: { ...cur.onboarding, completed: false, step: "welcome", plan: null },
                  }))
                }
              >
                Restart onboarding
              </button>
              <button
                className="pv__btn"
                type="button"
                onClick={() =>
                  store.set((cur) => ({ onboarding: { ...cur.onboarding, completed: true } }))
                }
              >
                Skip to panel
              </button>
            </Row>
            <div className="pv__list">
              {["welcome", "reads", "permission", "plan", "connect", "ready"].map((st) => (
                <button
                  key={st}
                  type="button"
                  className={`pv__opt${
                    !s.onboarding.completed && s.onboarding.step === st ? " is-on" : ""
                  }`}
                  onClick={() =>
                    store.set((cur) => ({
                      onboarding: { ...cur.onboarding, completed: false, step: st },
                    }))
                  }
                >
                  {st}
                </button>
              ))}
            </div>
            <Row>
              <Toggle
                label="Accessibility granted"
                on={s.onboarding.axGranted}
                onChange={(on) =>
                  store.set((cur) => ({ onboarding: { ...cur.onboarding, axGranted: on } }))
                }
              />
            </Row>
            <p className="pv__hint">
              The permission step polls the machine, so flipping this mid-step shows the moment it
              turns green. "Open System Settings" grants it after a beat, the way the real one does.
            </p>
          </Group>

          <Group title="Reading">
            <div className="pv__list">
              {APPS.map((a) => (
                <button
                  key={a.bundle}
                  type="button"
                  className={`pv__opt${s.foreground === a.bundle ? " is-on" : ""}`}
                  onClick={() => store.set({ foreground: a.bundle })}
                >
                  {a.name}
                </button>
              ))}
            </div>
          </Group>

          <Group title="Key">
            <Seg
              value={s.keyRejected ? "rejected" : s.hasKey ? "ok" : "none"}
              options={[
                { id: "ok", label: "Connected" },
                { id: "none", label: "Missing" },
                { id: "rejected", label: "Rejected" },
              ]}
              onChange={(v) =>
                store.set({ hasKey: v !== "none", keyRejected: v === "rejected" })
              }
            />
            <p className="pv__hint">
              Missing shows the welcome warning; rejected is what a 401 looks like in Settings.
            </p>
          </Group>

          <Group title="Tracked state">
            <Row>
              <button
                className="pv__btn"
                type="button"
                onClick={() => store.set({ commitments: INITIAL.commitments, openLoops: INITIAL.openLoops })}
              >
                Full
              </button>
              <button
                className="pv__btn"
                type="button"
                onClick={() =>
                  store.set({ commitments: INITIAL.commitments.slice(0, 1), openLoops: [] })
                }
              >
                One item
              </button>
              <button
                className="pv__btn"
                type="button"
                onClick={() => store.set({ commitments: [], openLoops: [] })}
              >
                Empty
              </button>
            </Row>
            <p className="pv__hint">
              {s.commitments.length} due · {s.openLoops.length} waiting. Click a row in the panel to
              resolve it.
            </p>
          </Group>

          <Group title="Approvals">
            <Row>
              <button
                className="pv__btn"
                type="button"
                onClick={() => store.set({ approvals: INITIAL.approvals })}
              >
                One pending
              </button>
              <button className="pv__btn" type="button" onClick={() => store.set({ approvals: [] })}>
                None
              </button>
            </Row>
            <p className="pv__hint">
              Everything leaving the device stops here (L3). The section hides itself when the queue
              is empty.
            </p>
          </Group>

          <Group title="Connections">
            <div className="pv__conns">
              {s.connections.map((c) => (
                <div key={c.source} className="pv__connrow">
                  <span>{c.source}</span>
                  <select
                    className="pv__select"
                    value={c.state}
                    onChange={(e) =>
                      store.set({
                        connections: s.connections.map((x) =>
                          x.source === c.source
                            ? { ...x, state: e.target.value as ConnState }
                            : x,
                        ),
                      })
                    }
                  >
                    {CONN_STATES.map((st) => (
                      <option key={st} value={st}>
                        {st}
                      </option>
                    ))}
                  </select>
                </div>
              ))}
            </div>
          </Group>

          <Group title="Nightly review">
            <Seg
              value={s.dream.indicator}
              options={[
                { id: "normal", label: "Ran" },
                { id: "amber", label: "Carried" },
                { id: "red", label: "Stalled" },
              ]}
              onChange={(v) =>
                store.set({
                  dream: {
                    ...s.dream,
                    indicator: v as "normal" | "amber" | "red",
                    last_succeeded: v === "normal",
                  },
                })
              }
            />
            <Row>
              <Toggle
                label="Batch lane"
                on={s.dream.batch_lane}
                onChange={(on) => store.set({ dream: { ...s.dream, batch_lane: on } })}
              />
            </Row>
          </Group>

          <Group title="Latency">
            <Seg
              value={String(s.latencyMs)}
              options={[
                { id: "0", label: "0ms" },
                { id: "60", label: "60ms" },
                { id: "400", label: "400ms" },
              ]}
              onChange={(v) => store.set({ latencyMs: Number(v) })}
            />
            <p className="pv__hint">
              Simulated IPC round-trip. 400ms is the "slow device" read of every settings toggle.
            </p>
          </Group>
        </aside>
      ) : null}
    </div>
  );
}

/** The window underneath. Its only job is to give the panel's glass something to blur. */
function DeskBackdrop(props: { app: DeskApp }): JSX.Element {
  return (
    <div className="pv__wall" aria-hidden="true">
      <div className="pv__blob pv__blob--a" />
      <div className="pv__blob pv__blob--b" />
      <div className="pv__win">
        <div className="pv__wintitle">
          <span className="pv__dots">
            <i /> <i /> <i />
          </span>
          {props.app.name}
        </div>
        <div className="pv__winbody">
          <div className="pv__sidebar">
            {Array.from({ length: 7 }).map((_, i) => (
              <span key={i} style={{ width: `${52 + ((i * 37) % 44)}%` }} />
            ))}
          </div>
          <div className="pv__doc">
            {Array.from({ length: 16 }).map((_, i) => (
              <span key={i} style={{ width: `${34 + ((i * 53) % 62)}%` }} />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function Group(props: { title: string; children: React.ReactNode }): JSX.Element {
  return (
    <section className="pv__group">
      <h2 className="pv__grouptitle">{props.title}</h2>
      {props.children}
    </section>
  );
}

function Row(props: { children: React.ReactNode }): JSX.Element {
  return <div className="pv__row">{props.children}</div>;
}

function Seg(props: {
  value: string;
  options: Array<{ id: string; label: string }>;
  onChange: (v: string) => void;
}): JSX.Element {
  return (
    <div className="pv__seg">
      {props.options.map((o) => (
        <button
          key={o.id}
          type="button"
          className={`pv__segopt${props.value === o.id ? " is-on" : ""}`}
          onClick={() => props.onChange(o.id)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function Toggle(props: { label: string; on: boolean; onChange: (on: boolean) => void }): JSX.Element {
  return (
    <button
      type="button"
      className={`pv__toggle${props.on ? " is-on" : ""}`}
      aria-pressed={props.on}
      onClick={() => props.onChange(!props.on)}
    >
      <span className="pv__knob" />
      {props.label}
    </button>
  );
}
