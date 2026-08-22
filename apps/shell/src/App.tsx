import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Logo } from "./Logo";
import { TitleBar } from "./TitleBar";
import { copy } from "./strings";
import { PANE_CHROME, type PaneId, type ShellView } from "./types";

const IN_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Appearance = "auto" | "light" | "dark";

function loadAppearance(): Appearance {
  try {
    const v = JSON.parse(localStorage.getItem("shogun.appearance") ?? '"auto"') as unknown;
    return v === "light" || v === "dark" ? v : "auto";
  } catch {
    return "auto";
  }
}

function applyAppearance(a: Appearance): void {
  document.documentElement.dataset.appearance = a;
  try {
    localStorage.setItem("shogun.appearance", JSON.stringify(a));
  } catch {
    /* private mode */
  }
}

const NAV: { id: PaneId; label: string; group?: string }[] = [
  { id: "today", label: copy.navToday },
  { id: "health", label: copy.navHealth },
  { id: "sources", label: copy.navSources, group: copy.groupContext },
  { id: "memory", label: copy.navMemory },
  { id: "activity", label: copy.navActivity, group: copy.groupDid },
  { id: "trace", label: copy.navTrace },
  { id: "settings", label: copy.navSettings },
];

export function App(): JSX.Element {
  const [view, setView] = useState<ShellView | null>(null);
  const [failed, setFailed] = useState<string | null>(
    IN_TAURI ? null : "Open this window from the ShogunAI app to load system status.",
  );
  const [pane, setPane] = useState<PaneId>("today");
  const [maximized, setMaximized] = useState(false);
  const [appearance, setAppearance] = useState<Appearance>(loadAppearance);
  const [autostart, setAutostart] = useState(false);
  const [autostartBusy, setAutostartBusy] = useState(false);

  useEffect(() => {
    applyAppearance(appearance);
  }, [appearance]);

  useEffect(() => {
    if (!IN_TAURI) return;
    invoke<ShellView>("shell_view")
      .then(setView)
      .catch((e) => setFailed(String(e)));
    invoke<boolean>("autostart_get")
      .then(setAutostart)
      .catch(() => setAutostart(false));
  }, []);

  useEffect(() => {
    if (!IN_TAURI) return;
    const win = getCurrentWindow();
    let stop: (() => void) | undefined;
    void win.isMaximized().then(setMaximized);
    void win.onResized(() => {
      void win.isMaximized().then(setMaximized);
    }).then((un) => {
      stop = un;
    });
    return () => {
      stop?.();
    };
  }, []);

  const onAutostart = (enabled: boolean) => {
    if (!IN_TAURI || autostartBusy) return;
    setAutostartBusy(true);
    invoke<boolean>("autostart_set", { enabled })
      .then(setAutostart)
      .catch(() => undefined)
      .finally(() => setAutostartBusy(false));
  };

  const head =
    pane === "settings"
      ? { title: copy.navSettings, sub: copy.settingsSub }
      : PANE_CHROME[pane];

  return (
    <div className="shell">
      <TitleBar maximized={maximized} />
      <div className="full">
        <div className="full__body">
          <nav className="side">
            <div className="side__brand">
              <Logo size={24} />
              <span className="side__name">{copy.product}</span>
            </div>
            {NAV.map((n) => (
              <div key={n.id}>
                {n.group && <div className="side__group">{n.group}</div>}
                <button
                  type="button"
                  className={`side__item${pane === n.id ? " is-on" : ""}`}
                  aria-current={pane === n.id ? "page" : undefined}
                  onClick={() => setPane(n.id)}
                >
                  {n.label}
                </button>
              </div>
            ))}
          </nav>
          <section className="pane">
            <div className="pane__head" key={pane}>
              <div className="pane__title">{head.title}</div>
              <div className="pane__sub">{head.sub}</div>
            </div>
            <div className="pane__body">
              {failed && pane !== "settings" && (
                <div className="fcard">
                  <div className="fempty">
                    {copy.bootFailed} — {failed}
                  </div>
                </div>
              )}
              {!failed && view && pane !== "settings" ? (
                <div className="fcard">
                  <div className="fempty">{paneBody(view, pane)}</div>
                </div>
              ) : null}
              {pane === "settings" && (
                <Settings
                  view={view}
                  appearance={appearance}
                  onAppearance={setAppearance}
                  autostart={autostart}
                  autostartBusy={autostartBusy}
                  onAutostart={onAutostart}
                />
              )}
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}

function paneBody(view: ShellView, pane: Exclude<PaneId, "settings">): string {
  return view[pane].body;
}

function Settings({
  view,
  appearance,
  onAppearance,
  autostart,
  autostartBusy,
  onAutostart,
}: {
  view: ShellView | null;
  appearance: Appearance;
  onAppearance: (a: Appearance) => void;
  autostart: boolean;
  autostartBusy: boolean;
  onAutostart: (enabled: boolean) => void;
}): JSX.Element {
  return (
    <>
      <div className="fcard">
        <div className="fcard__label">{copy.appearance}</div>
        <div className="seg" role="radiogroup" aria-label={copy.appearance}>
          {(
            [
              ["auto", copy.appearanceAuto],
              ["dark", copy.appearanceDark],
              ["light", copy.appearanceLight],
            ] as const
          ).map(([id, label]) => (
            <button
              key={id}
              type="button"
              role="radio"
              aria-checked={appearance === id}
              className={`seg__btn${appearance === id ? " is-on" : ""}`}
              onClick={() => onAppearance(id)}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      <div className="fcard">
        <div className="fcard__label">{copy.launchAtLogin}</div>
        <div className="frow">
          <div className="frow__lead">
            <div className="frow__t">{copy.launchAtLogin}</div>
            <div className="frow__d">{copy.launchAtLoginHint}</div>
          </div>
          <label className="toggle">
            <input
              type="checkbox"
              checked={autostart}
              disabled={autostartBusy || !IN_TAURI}
              onChange={(e) => onAutostart(e.target.checked)}
            />
            <span />
          </label>
        </div>
      </div>

      <div className="fcard">
        <div className="fcard__label">{copy.closeBehavior}</div>
        <div className="fempty">{view?.close_behavior ?? copy.closeBehavior}</div>
      </div>

      <div className="fcard">
        <div className="fcard__label">{copy.secrets}</div>
        <div className="frow">
          <div className="frow__lead">
            <div className="frow__t">{view?.secrets_backend ?? "—"}</div>
            <div className="frow__d">{view?.secrets_detail ?? ""}</div>
          </div>
          <span className={`pill${view?.secrets_ready ? " pill--ok" : ""}`}>
            {view?.secrets_ready ? "Ready" : "Not yet"}
          </span>
        </div>
      </div>

      <div className="fcard">
        <div className="fcard__label">{copy.dataFolder}</div>
        <div className="frow">
          <div className="frow__lead">
            <div className="frow__t">{copy.dataFolder}</div>
            <div className="frow__d frow__d--path">{view?.app_data_dir ?? "—"}</div>
          </div>
          <button
            type="button"
            className="btn"
            disabled={!IN_TAURI}
            onClick={() => void invoke("open_app_data_dir")}
          >
            {copy.openFolder}
          </button>
        </div>
      </div>
    </>
  );
}
