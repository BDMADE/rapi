import { describe, expect, it } from "vitest";
import { cn, formatUptime } from "./utils";

describe("UI utilities", () => {
  it("formats uptime across seconds, minutes, and hours", () => {
    expect(formatUptime(42)).toBe("42s");
    expect(formatUptime(125)).toBe("2m 5s");
    expect(formatUptime(3720)).toBe("1h 2m");
  });

  it("merges Tailwind classes deterministically", () => {
    expect(cn("px-2 py-1", "px-4")).toBe("py-1 px-4");
  });
});
