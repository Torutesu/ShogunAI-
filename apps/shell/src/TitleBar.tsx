import { getCurrentWindow } from "@tauri-apps/api/window";
import { copy } from "./strings";

const IN_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function TitleBar({ maximized }: { maximized: boolean }): JSX.Element {
  const win = IN_TAURI ? getCurrentWindow() : null;

  return (
    <div className="titlebar">
      <div
        className="titlebar__drag"
        data-tauri-drag-region
        onDoubleClick={() => {
          void win?.toggleMaximize();
        }}
      />
      <div className="titlebar__controls" role="group" aria-label="Window">
        <button
          type="button"
          className="caption caption--min"
          aria-label={copy.minimize}
          onClick={() => void win?.minimize()}
        >
          <CaptionMin />
        </button>
        <button
          type="button"
          className="caption caption--max"
          aria-label={maximized ? copy.restore : copy.maximize}
          onClick={() => void win?.toggleMaximize()}
        >
          {maximized ? <CaptionRestore /> : <CaptionMax />}
        </button>
        <button
          type="button"
          className="caption caption--close"
          aria-label={copy.close}
          onClick={() => void win?.close()}
        >
          <CaptionClose />
        </button>
      </div>
    </div>
  );
}

function CaptionMin(): JSX.Element {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
      <path d="M1 5h8" fill="none" stroke="currentColor" strokeWidth="1" />
    </svg>
  );
}

function CaptionMax(): JSX.Element {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
      <rect x="1.5" y="1.5" width="7" height="7" fill="none" stroke="currentColor" strokeWidth="1" />
    </svg>
  );
}

function CaptionRestore(): JSX.Element {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
      <path d="M3 3.5h4.5V8" fill="none" stroke="currentColor" strokeWidth="1" />
      <rect x="1.5" y="4.5" width="5" height="4" fill="none" stroke="currentColor" strokeWidth="1" />
    </svg>
  );
}

function CaptionClose(): JSX.Element {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
      <path d="M2 2l6 6M8 2L2 8" fill="none" stroke="currentColor" strokeWidth="1" />
    </svg>
  );
}
