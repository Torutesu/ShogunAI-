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
  it("stores a Groq key through the dedicated command and explains raw fallback", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_voice_settings") {
        return { enabled: false, share_personal_dictionary_with_speech_provider: false };
      }
      if (command === "get_voice_edit_settings") return { model: "openai/gpt-oss-120b", has_key: false };
      if (command === "set_voice_edit_key" || command === "focus_field") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<VoiceSection />);
    await screen.findByText("Not set — local vocabulary and raw transcript only.");
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

  it("adds and removes a local vocabulary term", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_voice_settings") {
        return { enabled: false, share_personal_dictionary_with_speech_provider: false };
      }
      if (command === "get_voice_edit_settings") return { model: "openai/gpt-oss-120b", has_key: false };
      if (command === "list_voice_dictionary_terms") return [];
      if (command === "create_voice_dictionary_term") {
        return { id: 7, ...(args as { term: object }).term, locale: null, scope: "global", scope_ref: null, priority: 0, provenance: "user" };
      }
      if (command === "delete_voice_dictionary_term") return true;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<VoiceSection />);
    fireEvent.change(screen.getByPlaceholderText("Correct spelling"), { target: { value: "ShogunAI" } });
    fireEvent.click(screen.getByRole("button", { name: "Add term" }));

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("create_voice_dictionary_term", {
        term: {
          canonical: "ShogunAI",
          aliases: [],
          locale: null,
          scope: "global",
          scope_ref: null,
          priority: 0,
          enabled: true,
        },
      });
    });
    expect(screen.getByText("ShogunAI")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Remove ShogunAI" }));
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("delete_voice_dictionary_term", { id: 7 });
    });
  });

  it("keeps personal vocabulary local until consent is granted", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_voice_settings") {
        return { enabled: false, share_personal_dictionary_with_speech_provider: false };
      }
      if (command === "get_voice_edit_settings") {
        return { model: "openai/gpt-oss-120b", has_key: false };
      }
      if (command === "list_voice_dictionary_terms") return [];
      if (command === "set_voice_dictionary_egress_consent") return undefined;
      throw new Error(`unexpected command: ${command}`);
    });

    render(<VoiceSection />);
    expect(screen.getByText(/Off by default. Personal vocabulary stays on this Mac/i)).toBeTruthy();

    fireEvent.click(
      screen.getByRole("checkbox", {
        name: "I allow SHOGUN to send eligible personal vocabulary terms to my speech provider as recognition hints.",
      }),
    );
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "set_voice_dictionary_egress_consent",
        { consent: true },
      );
    });
  });

  it("keeps the consent checkbox unchanged and shows a save error", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_voice_settings") return { enabled: false, share_personal_dictionary_with_speech_provider: false };
      if (command === "get_voice_edit_settings") return { model: "openai/gpt-oss-120b", has_key: false };
      if (command === "list_voice_dictionary_terms") return [];
      if (command === "set_voice_dictionary_egress_consent") throw new Error("write failed");
      throw new Error(`unexpected command: ${command}`);
    });

    render(<VoiceSection />);
    const checkbox = screen.getByRole("checkbox", { name: /I allow SHOGUN/i });
    fireEvent.click(checkbox);
    expect(checkbox).not.toBeChecked();
    expect(await screen.findByText("Couldn’t save vocabulary sharing. Your choice was not changed.")).toBeTruthy();
  });

  it("shows a retryable error when personal vocabulary cannot load", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_voice_settings") {
        return { enabled: false, share_personal_dictionary_with_speech_provider: false };
      }
      if (command === "get_voice_edit_settings") return { model: "openai/gpt-oss-120b", has_key: false };
      if (command === "list_voice_dictionary_terms") throw new Error("database unavailable");
      throw new Error(`unexpected command: ${command}`);
    });

    render(<VoiceSection />);
    expect(await screen.findByText("Couldn't load personal vocabulary.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
    expect(screen.queryByText("No personal terms yet. Add a spelling that speech often gets wrong.")).toBeNull();
  });

  it("edits an existing vocabulary term without dropping its scope metadata", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const existing = {
      id: 7,
      canonical: "Figma",
      aliases: ["fig ma", "Figma"],
      locale: "en-US",
      scope: "bundle",
      scope_ref: "com.figma.Desktop",
      priority: 8,
      enabled: true,
      provenance: "user",
    } as const;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_voice_settings") {
        return { enabled: false, share_personal_dictionary_with_speech_provider: false };
      }
      if (command === "get_voice_edit_settings") return { model: "openai/gpt-oss-120b", has_key: false };
      if (command === "list_voice_dictionary_terms") return [existing];
      if (command === "update_voice_dictionary_term") {
        return { ...existing, ...(args as { term: object }).term };
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<VoiceSection />);
    await screen.findByText("Figma");
    fireEvent.click(screen.getByRole("button", { name: "Edit Figma" }));
    fireEvent.change(screen.getByPlaceholderText("Correct spelling"), { target: { value: "Figma Design" } });
    fireEvent.click(screen.getByRole("button", { name: "Save changes" }));

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("update_voice_dictionary_term", {
        id: 7,
        term: {
          canonical: "Figma Design",
          aliases: ["fig ma"],
          locale: "en-US",
          scope: "bundle",
          scope_ref: "com.figma.Desktop",
          priority: 8,
          enabled: true,
        },
      });
    });
  });

  it("creates a scoped, localized vocabulary term with priority", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_voice_settings") {
        return { enabled: false, share_personal_dictionary_with_speech_provider: false };
      }
      if (command === "get_voice_edit_settings") return { model: "openai/gpt-oss-120b", has_key: false };
      if (command === "list_voice_dictionary_terms") return [];
      if (command === "create_voice_dictionary_term") {
        return { id: 8, ...(args as { term: object }).term, provenance: "user" };
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<VoiceSection />);
    fireEvent.change(screen.getByPlaceholderText("Correct spelling"), { target: { value: "Figma" } });
    fireEvent.click(screen.getByText("Language, app, and priority"));
    fireEvent.change(screen.getByLabelText("Language"), { target: { value: "en-US" } });
    fireEvent.change(screen.getByLabelText("Applies in"), { target: { value: "bundle" } });
    fireEvent.change(screen.getByLabelText("Scope identifier"), { target: { value: "com.figma.Desktop" } });
    fireEvent.change(screen.getByLabelText("Priority"), { target: { value: "8" } });
    fireEvent.click(screen.getByRole("button", { name: "Add term" }));

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("create_voice_dictionary_term", {
        term: {
          canonical: "Figma",
          aliases: [],
          locale: "en-US",
          scope: "bundle",
          scope_ref: "com.figma.Desktop",
          priority: 8,
          enabled: true,
        },
      });
    });
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
