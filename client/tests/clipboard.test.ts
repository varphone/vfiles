import { afterEach, describe, expect, it, vi } from "vitest";
import { copyText } from "../src/utils/clipboard";

function stubClipboard(writeText: () => Promise<void> | undefined) {
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText },
    configurable: true,
  });
}

afterEach(() => {
  Object.defineProperty(navigator, "clipboard", {
    value: undefined,
    configurable: true,
  });
  vi.restoreAllMocks();
});

describe("copyText", () => {
  it("uses the async clipboard API when available", async () => {
    const writeText = vi.fn(async () => {});
    stubClipboard(writeText);

    await expect(copyText("hello")).resolves.toBe(true);
    expect(writeText).toHaveBeenCalledWith("hello");
  });

  it("returns false for empty input without touching the clipboard", async () => {
    const writeText = vi.fn(async () => {});
    stubClipboard(writeText);

    await expect(copyText("")).resolves.toBe(false);
    expect(writeText).not.toHaveBeenCalled();
  });

  it("falls back to execCommand when the clipboard API is rejected", async () => {
    stubClipboard(() => Promise.reject(new Error("denied")));
    const execCommand = vi.fn(() => true);
    (document as unknown as { execCommand: unknown }).execCommand = execCommand;

    await expect(copyText("fallback")).resolves.toBe(true);
    expect(execCommand).toHaveBeenCalledWith("copy");
    // 回退用的临时节点不应留在 DOM 中
    expect(document.querySelector("textarea")).toBeNull();
  });

  it("reports failure when no mechanism works", async () => {
    Object.defineProperty(navigator, "clipboard", {
      value: undefined,
      configurable: true,
    });
    (document as unknown as { execCommand: unknown }).execCommand = () => {
      throw new Error("unsupported");
    };

    await expect(copyText("nope")).resolves.toBe(false);
    expect(document.querySelector("textarea")).toBeNull();
  });
});
