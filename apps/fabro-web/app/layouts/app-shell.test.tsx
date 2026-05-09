import { describe, expect, test } from "bun:test";
import { getVisibleNavigation } from "./app-shell";

describe("getVisibleNavigation", () => {
  test("shows all nav items in demo mode with Start first", () => {
    const items = getVisibleNavigation(true);
    const names = items.map((i) => i.name);
    expect(names[0]).toBe("Start");
    expect(names).toContain("Workflows");
    expect(names).toContain("Runs");
    expect(names).toContain("Insights");
    expect(names).toContain("Settings");
  });

  test("hides Start, Workflows, and Insights in production mode", () => {
    const items = getVisibleNavigation(false);
    const names = items.map((i) => i.name);
    expect(names).not.toContain("Start");
    expect(names).not.toContain("Workflows");
    expect(names).not.toContain("Insights");
    expect(names).toContain("Runs");
    expect(names).toContain("Settings");
  });
});
