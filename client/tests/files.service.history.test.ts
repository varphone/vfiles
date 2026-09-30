import { afterEach, describe, expect, it, vi } from "vitest";
import { filesService } from "../src/services/files.service";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("filesService.getFileDiff", () => {
  it("passes the abort signal through to the HTTP request", async () => {
    const controller = new AbortController();
    let requestSignal: AbortSignal | undefined;
    const fetchMock = vi.fn((_input: RequestInfo | URL, init?: RequestInit) => {
      requestSignal = init?.signal as AbortSignal | undefined;
      return new Promise<Response>((_resolve, reject) => {
        requestSignal?.addEventListener(
          "abort",
          () => reject(new DOMException("Aborted", "AbortError")),
          { once: true },
        );
      });
    });
    vi.stubGlobal("fetch", fetchMock);

    const pending = filesService.getFileDiff(
      "notes.txt",
      "commit-1",
      "parent-1",
      { signal: controller.signal },
    );
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/history/diff?path=notes.txt&commit=commit-1&parent=parent-1",
      expect.objectContaining({
        credentials: "include",
        signal: controller.signal,
      }),
    );

    controller.abort();

    await expect(pending).rejects.toMatchObject({ name: "AbortError" });
    expect(requestSignal?.aborted).toBe(true);
  });
});
