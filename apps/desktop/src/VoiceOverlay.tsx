// Hold-to-talk voice dialogue overlay (#44). Bottom-center floating panel: level bar while
// recording, spinner while processing, answer text with Copy/Close on response.

import { useCallback, useEffect, useRef, useState, type JSX } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { t } from "./strings";
import { uiLog } from "./uiLog";

interface VoiceStateEvent {
  phase: "idle" | "recording" | "processing" | "response" | "error";
  transcript?: string | null;
  response?: string | null;
}

interface LevelEvent {
  rms: number;
}

interface VoiceResponseEvent {
  text: string;
  transcript: string;
}

interface VoiceErrorEvent {
  message: string;
}

export function VoiceOverlay(): JSX.Element {
  const [phase, setPhase] = useState<VoiceStateEvent["phase"]>("idle");
  const [level, setLevel] = useState(0);
  const [transcript, setTranscript] = useState("");
  const [response, setResponse] = useState("");
  const [error, setError] = useState("");
  const peak = useRef(0);

  useEffect(() => {
    const unsubs: Array<() => void> = [];
    void listen<VoiceStateEvent>("voice_state", (e) => {
      const p = e.payload.phase;
      setPhase(p);
      if (e.payload.transcript) setTranscript(e.payload.transcript);
      if (e.payload.response) setResponse(e.payload.response);
      if (p === "recording") {
        setError("");
        setResponse("");
        peak.current = 0;
      }
      if (p === "idle") {
        setTranscript("");
        setResponse("");
        setError("");
        setLevel(0);
      }
    }).then((u) => unsubs.push(u));

    void listen<LevelEvent>("voice_level", (e) => {
      const rms = e.payload.rms;
      peak.current = Math.max(peak.current * 0.85, rms);
      const norm = peak.current > 0 ? Math.min(1, rms / peak.current) : 0;
      setLevel(norm);
    }).then((u) => unsubs.push(u));

    void listen<VoiceResponseEvent>("voice_response", (e) => {
      setTranscript(e.payload.transcript);
      setResponse(e.payload.text);
      setPhase("response");
    }).then((u) => unsubs.push(u));

    void listen<VoiceErrorEvent>("voice_error", (e) => {
      setError(e.payload.message);
      setPhase("error");
    }).then((u) => unsubs.push(u));

    return () => {
      for (const u of unsubs) u();
    };
  }, []);

  const dismiss = useCallback((): void => {
    void invoke("voice_dismiss").catch((err) => uiLog(`voice_dismiss failed: ${err}`));
  }, []);

  const copyResponse = useCallback((): void => {
    if (!response) return;
    void navigator.clipboard.writeText(response).catch(() => undefined);
  }, [response]);

  if (phase === "idle") {
    return <div className="voice-ov voice-ov--hidden" aria-hidden />;
  }

  return (
    <div className="voice-ov" role="dialog" aria-label={t.voiceTitle}>
      <div className="voice-ov__card ov__nodrag">
        {phase === "recording" ? (
          <>
            <div className="voice-ov__kicker">{t.voiceListening}</div>
            <div className="voice-ov__meter" aria-hidden>
              <div className="voice-ov__meter-fill" style={{ width: `${Math.round(level * 100)}%` }} />
            </div>
            <div className="voice-ov__hint">{t.voiceHoldHint}</div>
          </>
        ) : null}

        {phase === "processing" ? (
          <>
            <div className="voice-ov__kicker">{t.voiceProcessing}</div>
            {transcript ? <div className="voice-ov__transcript">"{transcript}"</div> : null}
          </>
        ) : null}

        {phase === "response" ? (
          <>
            <div className="voice-ov__kicker">{t.voiceAnswer}</div>
            {transcript ? <div className="voice-ov__transcript voice-ov__transcript--sub">"{transcript}"</div> : null}
            <div className="voice-ov__response">{response}</div>
            <div className="voice-ov__acts">
              <button type="button" className="voice-ov__btn" onClick={copyResponse}>
                {t.voiceCopy}
              </button>
              <button type="button" className="voice-ov__btn voice-ov__btn--primary" onClick={dismiss}>
                {t.voiceClose}
              </button>
            </div>
          </>
        ) : null}

        {phase === "error" ? (
          <>
            <div className="voice-ov__kicker voice-ov__kicker--err">{t.voiceError}</div>
            <div className="voice-ov__response">{error}</div>
            <div className="voice-ov__acts">
              <button type="button" className="voice-ov__btn voice-ov__btn--primary" onClick={dismiss}>
                {t.voiceClose}
              </button>
            </div>
          </>
        ) : null}
      </div>
    </div>
  );
}
