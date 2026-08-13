// Entry for the Visual recall browse window. Separate document from the notch panel — the
// timeline, image preview, and OCR scrubber need room a notch settings pane cannot give.

import React, { useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { resolveApp, segmentTint } from "./appIcons";
import { t } from "./strings";
import { IconClose } from "./utilityIcons";
import "./styles.css";

const IN_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Fixed spacing — history extends past the window; do not fit-all into one bar. */
const PX_PER_FRAME = 24;

// Browse window stays dark so scrub segments + playhead read clean.
document.documentElement.dataset.appearance = "dark";
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

type AppSegment = {
  start: number;
  end: number;
  app: string | null;
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

/** Contiguous runs of the same app_bundle_id (or null). */
function appSegments(frames: FrameListItem[]): AppSegment[] {
  if (frames.length === 0) return [];
  const out: AppSegment[] = [];
  let start = 0;
  let app = frames[0]!.app;
  for (let i = 1; i < frames.length; i++) {
    if (frames[i]!.app === app) continue;
    out.push({ start, end: i - 1, app });
    start = i;
    app = frames[i]!.app;
  }
  out.push({ start, end: frames.length - 1, app });
  return out;
}

/** Drag the browse window. Overlay title bar + CSS drag region; startDragging is the fallback. */
function beginDrag(e: React.PointerEvent): void {
  if (!IN_TAURI || e.button !== 0) return;
  const el = e.target as HTMLElement;
  if (el.closest("button, input, a, textarea, select, [data-no-drag]")) return;
  void getCurrentWindow().startDragging().catch(() => undefined);
}

/**
 * Present-center scrub: fixed playhead at viewport middle = selected time.
 * Track uses fixed px/frame so history runs off-window to the left; drag/scroll pans under center.
 */
function ScrubBar(props: {
  frames: FrameListItem[];
  value: number;
  label: string;
  onChange: (next: number) => void;
}): JSX.Element {
  const { frames, value, label, onChange } = props;
  const max = Math.max(0, frames.length - 1);
  const viewportRef = useRef<HTMLDivElement>(null);
  const [viewW, setViewW] = useState(0);
  const drag = useRef<{
    pointerId: number;
    startX: number;
    startIdx: number;
    moved: boolean;
  } | null>(null);

  const segments = useMemo(() => appSegments(frames), [frames]);

  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const sync = (): void => setViewW(el.clientWidth);
    sync();
    const ro = new ResizeObserver(sync);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const clampIdx = (n: number): number => Math.min(max, Math.max(0, Math.round(n)));

  /** Visual: left = older, right = newer. Fixed center playhead; rail pans underneath. */
  const seekFromClientX = (clientX: number, baseIdx: number): void => {
    const el = viewportRef.current;
    if (!el || max <= 0) return;
    const rect = el.getBoundingClientRect();
    const centerX = rect.left + rect.width / 2;
    // Tap left of center → older (lower idx).
    onChange(clampIdx(baseIdx + (clientX - centerX) / PX_PER_FRAME));
  };

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>): void => {
    if (e.button !== 0 || max <= 0) return;
    drag.current = { pointerId: e.pointerId, startX: e.clientX, startIdx: value, moved: false };
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>): void => {
    const d = drag.current;
    if (!d || e.pointerId !== d.pointerId) return;
    const dx = e.clientX - d.startX;
    if (!d.moved && Math.abs(dx) < 3) return;
    d.moved = true;
    // Drag right → rail follows finger → older under playhead (natural left-to-right pan).
    onChange(clampIdx(d.startIdx - dx / PX_PER_FRAME));
  };

  const onPointerUp = (e: React.PointerEvent<HTMLDivElement>): void => {
    const d = drag.current;
    if (!d || e.pointerId !== d.pointerId) return;
    drag.current = null;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
    // Tap (no drag): seek relative to fixed center.
    if (!d.moved) seekFromClientX(e.clientX, d.startIdx);
  };

  const onWheel = (e: React.WheelEvent<HTMLDivElement>): void => {
    if (max <= 0) return;
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
    if (delta === 0) return;
    e.preventDefault();
    // Scroll right (pos delta) → newer under playhead (left-to-right timeline feel).
    onChange(clampIdx(value + delta / PX_PER_FRAME));
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

  // Half-viewport pads so oldest/newest can sit under the fixed center marker.
  const pad = viewW / 2;
  const railW = pad * 2 + frames.length * PX_PER_FRAME;
  // Selected frame center under viewport center.
  const translateX = viewW > 0 ? -(value + 0.5) * PX_PER_FRAME : 0;

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
        ref={viewportRef}
        className="vr-scrub__viewport"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onWheel={onWheel}
      >
        <div
          className="vr-scrub__rail"
          style={{ width: railW, transform: `translate3d(${translateX}px, 0, 0)` }}
        >
          <div className="vr-scrub__pad" style={{ width: pad }} />
          <div className="vr-scrub__track" style={{ width: frames.length * PX_PER_FRAME }}>
            {segments.map((seg) => {
              const info = resolveApp(seg.app);
              const w = (seg.end - seg.start + 1) * PX_PER_FRAME;
              const active = value >= seg.start && value <= seg.end;
              const showMark = w >= 22;
              const initial = (info.label.charAt(0) || "?").toUpperCase();
              return (
                <div
                  key={`${seg.start}-${seg.end}`}
                  className={`vr-scrub__seg${active ? " vr-scrub__seg--active" : ""}`}
                  title={info.label}
                  style={{
                    left: seg.start * PX_PER_FRAME,
                    width: w,
                    background: segmentTint(info.color, active),
                  }}
                >
                  {showMark ? (
                    <span className="vr-scrub__seg-mark" aria-hidden>
                      {initial}
                    </span>
                  ) : null}
                </div>
              );
            })}
          </div>
          <div className="vr-scrub__pad" style={{ width: pad }} />
        </div>
        <div className="vr-scrub__playhead" aria-hidden />
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
  const centeredOnce = useRef(false);

  const refreshFrames = (): void => {
    if (!IN_TAURI) return;
    void invoke<FrameListItem[]>("list_screen_frames")
      .then((rows) => {
        // API newest-first → oldest→newest so index grows toward present.
        const ordered = [...rows].reverse();
        setFrames(ordered);
        setIdx((cur) => {
          if (ordered.length === 0) return 0;
          if (!centeredOnce.current) {
            centeredOnce.current = true;
            return ordered.length - 1;
          }
          return Math.min(cur, ordered.length - 1);
        });
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
  const appLabel = resolveApp(appName).label;

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
            <IconClose />
          </button>
        </div>
      </header>

      <main className="vr-body">
        {failed ? <p className="vr-body__err">{failed}</p> : null}

        {frames.length > 0 ? (
          <div className="vr-stage">
            {previewUrl && current ? (
              <figure className="vr-stage__preview">
                <div className="vr-stage__media">
                  <img className="vr-stage__img" src={previewUrl} alt="" draggable={false} />
                </div>
                <figcaption className="vr-stage__caption">
                  {when != null ? <time>{formatWhen(when)}</time> : null}
                  {appName ? <span className="vr-stage__app">{appLabel}</span> : null}
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
            {/* Scrub outside the preview card — present fixed at center, history pans left. */}
            <ScrubBar
              frames={frames}
              value={idx}
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
