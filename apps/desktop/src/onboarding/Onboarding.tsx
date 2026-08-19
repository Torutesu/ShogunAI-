import { useCallback, useEffect, useRef, useState } from "react";
import type { JSX } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  acknowledgeOnboardingRestart,
  EMPTY_PERMISSIONS,
  getOnboardingState,
  IN_TAURI,
  onboardingWindowSurface,
  permissionListenerReady,
  permissionStatus,
  setOnboardingMusicMuted,
  setOnboardingState,
  track,
} from "./ipc";
import type { OnboardingState, OnboardingWindowSurface, PermissionSnapshot } from "./ipc";
import { AmbientSurface } from "./experience/AmbientSurface";
import { CinematicSurface } from "./experience/CinematicSurface";
import { OnboardingExperience } from "./experience/OnboardingExperience";

type RouteSurface = "main" | "ambient" | "interactive";

export function newestPermissionSnapshot(current: PermissionSnapshot, incoming: PermissionSnapshot): PermissionSnapshot {
  return (incoming.revision ?? 0) >= (current.revision ?? 0) ? incoming : current;
}

export function windowRoute(search = window.location.search): { surface: RouteSurface; generation: number | null } {
  const params = new URLSearchParams(search);
  const requested = params.get("surface");
  const rawGeneration = params.get("generation");
  const generation = rawGeneration === null ? Number.NaN : Number(rawGeneration);
  return {
    surface: requested === "main" || requested === "ambient" || requested === "interactive" ? requested : "interactive",
    generation: Number.isSafeInteger(generation) && generation >= 0 ? generation : null,
  };
}

export function Onboarding(): JSX.Element {
  const [state, setState] = useState<OnboardingState | null>(null);
  const [permissions, setPermissions] = useState<PermissionSnapshot>(EMPTY_PERMISSIONS);
  const [surface, setSurface] = useState<OnboardingWindowSurface | null | undefined>(undefined);
  const restartAckRevision = useRef<number | null>(null);
  const route = windowRoute();

  useEffect(() => {
    let alive = true;
    void Promise.all([getOnboardingState(), permissionStatus()]).then(([nextState, nextPermissions]) => {
      if (!alive) return;
      setState(nextState);
      setPermissions((current) => newestPermissionSnapshot(current, nextPermissions));
      track("shown");
    });
    if (IN_TAURI && route.generation !== null) {
      void onboardingWindowSurface(route.generation).then((nativeSurface) => { if (alive) setSurface(nativeSurface); });
    } else {
      setSurface({ surface: route.surface, generation: route.generation ?? 0, display_id: 0, label: "preview" });
    }
    if (!IN_TAURI) return () => { alive = false; };
    const listeners: Array<Promise<() => void>> = [
      listen<PermissionSnapshot>("permissions-changed", (event) => setPermissions((current) => newestPermissionSnapshot(current, event.payload))).then((off) => {
        if (alive) void permissionListenerReady().then((snapshot) => { if (snapshot) setPermissions((current) => newestPermissionSnapshot(current, snapshot)); });
        return off;
      }),
    ];
    return () => { alive = false; listeners.forEach((listener) => void listener.then((off) => off())); };
  }, [route.generation, route.surface]);

  useEffect(() => {
    if (!state?.restart_pending || state.step !== "screen_recording" || !permissions.screen_recording || restartAckRevision.current === state.revision) return;
    restartAckRevision.current = state.revision;
    void acknowledgeOnboardingRestart(state).then((saved) => {
      if (saved) setState(saved);
      else restartAckRevision.current = null;
    });
  }, [permissions.screen_recording, state]);

  const persist = useCallback(async (step: OnboardingState["step"], patch: Partial<OnboardingState> = {}): Promise<boolean> => {
    if (!state) return false;
    const saved = await setOnboardingState({ ...state, ...patch, step, completed: false });
    if (!saved) return false;
    setState(saved);
    return true;
  }, [state]);
  const finish = useCallback(async (): Promise<boolean> => {
    if (!state || !permissions.all_effective) return false;
    const saved = await setOnboardingState({ ...state, step: "ready", completed: true, permissions_repair: false });
    if (!saved) return false;
    setState(saved);
    return true;
  }, [permissions.all_effective, state]);
  const toggleMusic = useCallback(async (): Promise<boolean> => {
    if (!state) return false;
    const saved = await setOnboardingMusicMuted(state, !state.music_muted);
    if (!saved) return false;
    setState(saved);
    return true;
  }, [state]);

  if (surface === undefined || !state) return <div className="onb-boot" />;
  if (!surface || surface.surface !== route.surface) return <div className="onb-stale" data-testid="stale-surface" />;
  if (surface.surface === "main") return <CinematicSurface />;
  if (surface.surface === "ambient") return <AmbientSurface />;
  return <OnboardingExperience state={state} permissions={permissions} surfaceGeneration={surface.generation} onPersist={persist} onFinish={finish} onToggleMusic={toggleMusic} />;
}
