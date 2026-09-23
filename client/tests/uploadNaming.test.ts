import { describe, expect, it } from "vitest";
import { keepBothName, splitName } from "../src/utils/uploadNaming";

describe("splitName", () => {
  it("splits base and extension, dotfiles stay whole", () => {
    expect(splitName("a.txt")).toEqual({ base: "a", ext: ".txt" });
    expect(splitName("a.b.c")).toEqual({ base: "a.b", ext: ".c" });
    expect(splitName(".env")).toEqual({ base: ".env", ext: "" });
    expect(splitName("README")).toEqual({ base: "README", ext: "" });
  });
});

describe("keepBothName", () => {
  it("returns original when free, then (1) (2) loop", () => {
    const taken = new Set<string>();
    expect(keepBothName("a.txt", taken)).toBe("a.txt");
    taken.add("a.txt");
    expect(keepBothName("a.txt", taken)).toBe("a (1).txt");
    taken.add("a (1).txt");
    expect(keepBothName("a.txt", taken)).toBe("a (2).txt");
  });

  it("handles multi-dot and collision-free names", () => {
    const taken = new Set(["a.b.txt", "a.b (1).txt"]);
    expect(keepBothName("a.b.txt", taken)).toBe("a.b (2).txt");
    expect(keepBothName("c.log", taken)).toBe("c.log");
  });
});
