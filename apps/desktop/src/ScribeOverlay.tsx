import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "./strings";

interface ScribeEvent {
  session_id: number;
  phase: "opened" | "processing" | "inserted" | "failed" | "closed" | "cancelled" | "no_key";
  chars: number;
  detail: string | null;
}

export function sessionFromUrl(search = window.location.search): number | null {
  const value = new URLSearchParams(search).get("session");
  if (!value) return null;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : null;
}

export function ScribeOverlay(): JSX.Element {
  const [instruction, setInstruction] = useState("");
  const [phase, setPhase] = useState<"idle" | "processing" | "error">("idle");
  const [error, setError] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);
  const sessionId = useRef(sessionFromUrl());
  const closing = useRef(false);
  const pendingInstruction = useRef("");

  const focusEditBox = useCallback((): void => {
    void getCurrentWindow()
      .setFocus()
      .catch(() => undefined)
      .finally(() => {
        window.requestAnimationFrame(() => inputRef.current?.focus());
        window.setTimeout(() => inputRef.current?.focus(), 50);
      });
  }, []);

  const applyStatus = useCallback(
    (payload: ScribeEvent): void => {
      if (payload.session_id !== sessionId.current) return;
      if (payload.phase === "processing") {
        setPhase("processing");
      } else if (payload.phase === "inserted") {
        pendingInstruction.current = "";
        setInstruction("");
        setPhase("idle");
        setError("");
        focusEditBox();
      } else if (payload.phase === "failed" || payload.phase === "no_key") {
        const pending = pendingInstruction.current;
        pendingInstruction.current = "";
        setInstruction(pending);
        setPhase("error");
        setError(payload.detail || t.scribeError);
        focusEditBox();
      } else if (payload.phase === "closed" || payload.phase === "cancelled") {
        closing.current = true;
        void getCurrentWindow().close();
      }
    },
    [focusEditBox],
  );

  const close = useCallback(async (): Promise<void> => {
    if (closing.current) return;
    closing.current = true;
    const id = sessionId.current;
    if (id != null) {
      await invoke("scribe_close", { sessionId: id }).catch(() => undefined);
    }
    await getCurrentWindow().close().catch(() => undefined);
  }, []);

  const submit = useCallback((): void => {
    const text = instruction.trim();
    const id = sessionId.current;
    if (!text || id == null || phase === "processing") return;
    pendingInstruction.current = text;
    setInstruction("");
    setPhase("processing");
    setError("");
    void invoke("scribe_submit", { sessionId: id, instruction: text }).catch((reason) => {
      pendingInstruction.current = "";
      setInstruction(text);
      setPhase("error");
      setError(typeof reason === "string" ? reason : t.scribeError);
      focusEditBox();
    });
  }, [focusEditBox, instruction, phase]);

  useEffect(() => {
    if (sessionId.current == null) {
      void getCurrentWindow().close();
      return;
    }
    focusEditBox();
    const unlisten = listen<ScribeEvent>("scribe", ({ payload }) => applyStatus(payload));
    const unlistenClose = getCurrentWindow().onCloseRequested((event) => {
      if (closing.current) return;
      event.preventDefault();
      void close();
    });
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      void close();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      void unlisten.then((off) => off());
      void unlistenClose.then((off) => off());
    };
  }, [applyStatus, close, focusEditBox]);

  useEffect(() => {
    if (phase !== "processing") return;
    const id = sessionId.current;
    if (id == null) return;
    let stopped = false;
    const poll = (): void => {
      void invoke<ScribeEvent>("scribe_status", { sessionId: id })
        .then((status) => {
          if (!stopped && status.phase !== "processing") applyStatus(status);
        })
        .catch(() => undefined);
    };
    const timer = window.setInterval(poll, 200);
    poll();
    return () => {
      stopped = true;
      window.clearInterval(timer);
    };
  }, [applyStatus, phase]);

  return (
    <main className="scribe-float" role="dialog" aria-label={t.scribeTitle} title={error || undefined}>
      <div className={`scribe-float__field${phase === "error" ? " is-error" : ""}`}>
        <input
          ref={inputRef}
          className="scribe-float__input"
          aria-label={t.scribeLabel}
          aria-invalid={phase === "error"}
          placeholder={phase === "error" ? error : t.scribePlaceholder}
          value={instruction}
          disabled={phase === "processing"}
          onChange={(event) => {
            setInstruction(event.target.value);
            if (phase === "error") {
              setPhase("idle");
              setError("");
            }
          }}
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            submit();
          }}
        />
        <button
          className="scribe-float__submit"
          type="button"
          aria-label={phase === "processing" ? t.scribeProcessing : t.scribeSubmit}
          disabled={!instruction.trim() || phase === "processing"}
          onClick={submit}
        >
          {phase === "processing" ? <span className="scribe-float__spinner" aria-hidden="true" /> : "↑"}
        </button>
      </div>
    </main>
  );
}
