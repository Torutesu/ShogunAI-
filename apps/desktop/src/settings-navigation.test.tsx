import { useState } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import {
  SettingsSectionNav,
  shouldMountSettingsSection,
  type SettingsSectionId,
} from "./App";

afterEach(cleanup);

function Harness(): JSX.Element {
  const [active, setActive] = useState<SettingsSectionId>("general");
  return <SettingsSectionNav active={active} onChange={setActive} />;
}

describe("settings section navigation", () => {
  it("mounts only the active section so opening Settings does not load every page", () => {
    expect(shouldMountSettingsSection("general", "general")).toBe(true);
    expect(shouldMountSettingsSection("general", "memory")).toBe(false);
    expect(shouldMountSettingsSection("voice", "voice")).toBe(true);
  });

  it("shows focused pages instead of one undifferentiated settings list", () => {
    render(<Harness />);

    expect(screen.getAllByRole("tab")).toHaveLength(7);
    expect(screen.getByRole("tab", { name: "General" }).getAttribute("aria-selected")).toBe("true");

    fireEvent.click(screen.getByRole("tab", { name: "Memory" }));
    expect(screen.getByRole("tab", { name: "Memory" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tab", { name: "General" }).getAttribute("aria-selected")).toBe("false");
  });

  it("supports arrow and boundary-key navigation", () => {
    render(<Harness />);

    const general = screen.getByRole("tab", { name: "General" });
    fireEvent.keyDown(general, { key: "ArrowDown" });
    expect(screen.getByRole("tab", { name: "Memory" }).getAttribute("aria-selected")).toBe("true");

    fireEvent.keyDown(screen.getByRole("tab", { name: "Memory" }), { key: "End" });
    expect(screen.getByRole("tab", { name: "Privacy" }).getAttribute("aria-selected")).toBe("true");

    fireEvent.keyDown(screen.getByRole("tab", { name: "Privacy" }), { key: "Home" });
    expect(screen.getByRole("tab", { name: "General" }).getAttribute("aria-selected")).toBe("true");
  });
});
