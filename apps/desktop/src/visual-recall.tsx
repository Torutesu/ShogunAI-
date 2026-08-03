// Entry for the Visual recall browse window. Separate document from the notch panel — the
// timeline, image preview, and OCR scrubber need room a notch settings pane cannot give.

import React, { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "./strings";
import "./styles.css";

const IN_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function loadAppearance(): "auto" | "light" | "dark" {
  try {
    const v = JSON.parse(localStorage.getItem("shogun.appearance") ?? '"auto"');
    return v === "light" || v === "dark" ? v : "auto";
  } catch {
    return "auto";
  }
}
document.documentElement.dataset.appearance = loadAppearance();
document.body.classList.add("full-window");

type FrameListItem = {
  id: number;
  ts: number;
  app: string | null;
  window: string | null;
  width: number;
  height: number;
  ocr_excerpt: string;
  source: string;
};

type FrameImage = {
  jpeg_base64: string;
  ocr_text: string;
  ts: number;
  app: string | null;
  window: string | null;
  source: string;
};

function formatWhen(ts: number): string {
  return new Date(ts).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function excerptOcr(text: string, maxLines = 2): string {
  const lines = text.trim().split(/\n/).filter(Boolean).slice(0, maxLines);
  return lines.join("\n");
}

/** Drag the browse window. Overlay title bar + CSS drag region; startDragging is the fallback. */
function beginDrag(e: React.PointerEvent): void {
  if (!IN_TAURI || e.button !== 0) return;
  const el = e.target as HTMLElement;
  if (el.closest("button, input, a, textarea, select, [data-no-drag]")) return;
  void getCurrentWindow().startDragging().catch(() => undefined);
}

/** Scrub strip lives outside the preview card — drag handle, click track, arrow keys. */
function ScrubBar(props: {
  value: number;
  max: number;
  label: string;
  onChange: (next: number) => void;
}): JSX.Element {
  const { value, max, label, onChange } = props;
  const trackRef = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);

  const seekFromClientX = (clientX: number): void => {
    const el = trackRef.current;
    if (!el || max <= 0) return;
    const rect = el.getBoundingClientRect();
    if (rect.width <= 0) return;
    const t = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
    onChange(Math.round(t * max));
  };

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>): void => {
    if (e.button !== 0 || max <= 0) return;
    dragging.current = true;
    e.currentTarget.setPointerCapture(e.pointerId);
    seekFromClientX(e.clientX);
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>): void => {
    if (!dragging.current) return;
    seekFromClientX(e.clientX);
  };

  const onPointerUp = (e: React.PointerEvent<HTMLDivElement>): void => {
    if (!dragging.current) return;
    dragging.current = false;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    if (max <= 0) return;
    if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
      e.preventDefault();
      onChange(Math.max(0, value - 1));
    } else if (e.key === "ArrowRight" || e.key === "ArrowUp") {
      e.preventDefault();
      onChange(Math.min(max, value + 1));
    } else if (e.key === "Home") {
      e.preventDefault();
      onChange(0);
    } else if (e.key === "End") {
      e.preventDefault();
      onChange(max);
    }
  };

  const pct = max <= 0 ? 0 : (value / max) * 100;

  return (
    <div
      className="vr-scrub"
      data-no-drag
      role="slider"
      tabIndex={0}
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={max}
      aria-valuenow={value}
      onKeyDown={onKeyDown}
    >
      <div
        ref={trackRef}
        className="vr-scrub__track"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      >
        <div className="vr-scrub__fill" style={{ width: `${pct}%` }} />
        <div className="vr-scrub__thumb" style={{ left: `${pct}%` }} />
      </div>
    </div>
  );
}

function VisualRecallBrowse(): JSX.Element {
  const [frames, setFrames] = useState<FrameListItem[]>([]);
  const [idx, setIdx] = useState(0);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [previewMeta, setPreviewMeta] = useState<FrameImage | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [ocrOpen, setOcrOpen] = useState(false);
  const [deleteArmed, setDeleteArmed] = useState(false);

  const refreshFrames = (): void => {
    if (!IN_TAURI) return;
    void invoke<FrameListItem[]>("list_screen_frames")
      .then((rows) => {
        const ordered = [...rows].reverse();
        setFrames(ordered);
        setIdx((cur) => (ordered.length === 0 ? 0 : Math.min(cur, ordered.length - 1)));
        setFailed(null);
      })
      .catch((e) => setFailed(String(e)));
  };

  useEffect(() => {
    refreshFrames();
    const id = window.setInterval(refreshFrames, 12_000);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    setOcrOpen(false);
    setDeleteArmed(false);
  }, [idx]);

  useEffect(() => {
    if (!IN_TAURI || frames.length === 0) {
      setPreviewUrl(null);
      setPreviewMeta(null);
      return;
    }
    const frame = frames[idx];
    if (!frame) return;
    let cancelled = false;
    void invoke<FrameImage>("get_screen_frame_image", { frameId: frame.id })
      .then((img) => {
        if (cancelled) return;
        setPreviewMeta(img);
        setPreviewUrl(`data:image/jpeg;base64,${img.jpeg_base64}`);
      })
      .catch(() => {
        if (!cancelled) {
          setPreviewUrl(null);
          setPreviewMeta(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [frames, idx]);

  const deleteCurrent = (): void => {
    if (!IN_TAURI || frames.length === 0) return;
    const frame = frames[idx];
    if (!frame) return;
    void invoke("delete_screen_frame", { frameId: frame.id })
      .then(() => {
        setDeleteArmed(false);
        refreshFrames();
      })
      .catch((e) => setFailed(String(e)));
  };

  const closeWindow = (): void => {
    if (!IN_TAURI) return;
    void getCurrentWindow().close();
  };

  const current = frames[idx];
  const ocrText = previewMeta?.ocr_text || current?.ocr_excerpt || "";
  const when = previewMeta?.ts ?? current?.ts;
  const appName = previewMeta?.app ?? current?.app;

  return (
    <div className="vr-shell">
      <header className="vr-chrome" onPointerDown={beginDrag}>
        <div className="vr-chrome__title">
          <span className="vr-chrome__name">{t.visualRecallSection}</span>
          <span className="vr-chrome__sub">{t.visualRecallTimeline}</span>
        </div>
        <div className="vr-chrome__actions" data-no-drag>
          {frames.length > 0 ? (
            deleteArmed ? (
              <div className="vr-chrome__confirm">
                <span className="vr-chrome__confirm-label">{t.visualRecallDeleteConfirm}</span>
                <button type="button" className="vr-chrome__btn vr-chrome__btn--danger" onClick={deleteCurrent}>
                  {t.visualRecallDeleteFrame}
                </button>
                <button type="button" className="vr-chrome__btn" onClick={() => setDeleteArmed(false)}>
                  {t.visualRecallDeleteCancel}
                </button>
              </div>
            ) : (
              <button
                type="button"
                className="vr-chrome__btn vr-chrome__btn--danger"
                onClick={() => setDeleteArmed(true)}
              >
                {t.visualRecallDeleteFrame}
              </button>
            )
          ) : null}
          <button type="button" className="vr-chrome__btn vr-chrome__btn--icon" onClick={closeWindow} aria-label={t.visualRecallClose}>
            ✕
          </button>
        </div>
      </header>

      <main className="vr-body">
        {failed ? <p className="vr-body__err">{failed}</p> : null}

        {frames.length > 0 ? (
          <div className="vr-stage">
            {previewUrl && current ? (
              <figure className="vr-stage__preview">
                <img src={previewUrl} alt="" draggable={false} />
                <figcaption className="vr-stage__caption">
                  {when != null ? <time>{formatWhen(when)}</time> : null}
                  {appName ? <span className="vr-stage__app">{appName}</span> : null}
                </figcaption>
                {ocrText ? (
                  <div className="vr-stage__ocr" data-no-drag>
                    <button
                      type="button"
                      className="vr-stage__ocr-toggle"
                      onClick={() => setOcrOpen((v) => !v)}
                    >
                      {ocrOpen ? t.visualRecallHideText : t.visualRecallShowText}
                    </button>
                    {ocrOpen ? <p className="vr-stage__ocr-text">{excerptOcr(ocrText)}</p> : null}
                  </div>
                ) : null}
              </figure>
            ) : (
              <div className="vr-stage__preview vr-stage__preview--empty" aria-hidden />
            )}
            {/* Scrub outside the preview card — not boxed into the same frame. */}
            <ScrubBar
              value={idx}
              max={Math.max(0, frames.length - 1)}
              label={t.visualRecallScrubHint}
              onChange={setIdx}
            />
          </div>
        ) : (
          <p className="vr-body__empty">{t.visualRecallTimelineEmpty}</p>
        )}
      </main>
    </div>
  );
}

function Root(): JSX.Element {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || !IN_TAURI) return;
      void getCurrentWindow().close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!IN_TAURI) {
    return (
      <div className="full-boot">
        Visual recall browse window — open from SHOGUN Settings while the app is running.
      </div>
    );
  }

  return <VisualRecallBrowse />;
}

const el = document.getElementById("root");
if (el) createRoot(el).render(<React.StrictMode><Root /></React.StrictMode>);
