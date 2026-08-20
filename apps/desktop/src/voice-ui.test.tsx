import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  NotchStatusSection,
  VoicePanel,
  VoicePill,
  VoiceProcessingPill,
  VoiceSection,
  GoogleOAuthSection,
  type VoiceView,
} from "./App";

const ERROR_VIEW: VoiceView = {
  phase: "error",
  transcript: "",
  response: "",
  error: "Microphone unavailable",
  level: 0,
};

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(cleanup);

describe("compact dictation UI", () => {
  it("renders recording bars and a distinct processing loader", () => {
    const { container, rerender } = render(<VoicePill />);
    expect(screen.getByRole("status", { name: "Listening…" })).toBeTruthy();
    expect(container.querySelectorAll(".vpill__bar")).toHaveLength(4);

    rerender(<VoiceProcessingPill />);
    expect(screen.getByRole("status", { name: "Transcribing…" })).toBeTruthy();
    expect(container.querySelector(".vpill__loader")).toBeTruthy();
  });

  it("keeps a terminal dictation error visible until Close", () => {
    const dismiss = vi.fn();
    render(<VoicePanel view={ERROR_VIEW} onDismiss={dismiss} />);

    expect(screen.getByText("Microphone unavailable")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(dismiss).toHaveBeenCalledOnce();
  });
});

describe("dictation cleanup settings", () => {
  it("lists microphones and persists the selected dictation input", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_voice_settings") return { enabled: true, microphone: null };
      if (command === "get_voice_microphones") return ["Built-in Microphone", "Studio Mic"];
      if (command === "get_voice_edit_settings") return { model: "openai/gpt-oss-120b", has_key: false };
      if (command === "set_voice_microphone") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<VoiceSection />);
    const picker = await screen.findByLabelText("Input microphone");
    expect(picker.closest(".mic-picker__control")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Refresh" })).toBeTruthy();
    fireEvent.click(picker);
    expect(screen.getByRole("dialog", { name: "Choose Input" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Studio Mic/ }));

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("set_voice_microphone", {
        microphone: "Studio Mic",
      });
    });
  });

  it("stores a Groq key through the dedicated command and explains raw fallback", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_voice_settings") return { enabled: false };
      if (command === "get_voice_microphones") return [];
      if (command === "get_voice_edit_settings") return { model: "openai/gpt-oss-120b", has_key: false };
      if (command === "set_voice_edit_key" || command === "focus_field") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<VoiceSection />);
    await screen.findByText("Not set — raw transcript only.");
    expect(screen.getByText(/sent to Groq for process-only formatting/i)).toBeTruthy();

    fireEvent.change(screen.getByPlaceholderText("Paste your Groq API key…"), {
      target: { value: "gsk_test" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("set_voice_edit_key", { key: "gsk_test" });
    });
    expect(screen.getByText("Connected — cleanup is on.")).toBeTruthy();
  });
});

describe("Google OAuth connector settings", () => {
  it("saves client credentials without reading them back into the UI", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "google_oauth_settings") {
        return { has_client_id: false, has_client_secret: false };
      }
      if (command === "set_google_oauth_client" || command === "focus_field") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<GoogleOAuthSection />);
    await screen.findByText(/No saved client/i);
    fireEvent.change(screen.getByPlaceholderText("Google OAuth client ID"), {
      target: { value: "desktop-client-id" },
    });
    fireEvent.change(screen.getByPlaceholderText("Google OAuth client secret (optional)"), {
      target: { value: "desktop-client-secret" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save client" }));

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("set_google_oauth_client", {
        clientId: "desktop-client-id",
        clientSecret: "desktop-client-secret",
      });
    });
    expect(screen.queryByDisplayValue("desktop-client-secret")).toBeNull();
  });
});

describe("notch status setting", () => {
  it("persists Hide with the existing backend command", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockResolvedValue(undefined);
    const setVisible = vi.fn();

    render(<NotchStatusSection visible={true} onVisibleChange={setVisible} />);
    fireEvent.click(screen.getByRole("radio", { name: "Hide" }));

    expect(setVisible).toHaveBeenCalledWith(false);
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("set_notch_status_visible", { visible: false });
    });
  });
});
