import { describe, expect, it, vi } from "vitest";
import { ref } from "vue";
import {
  detectPreviewKind,
  escapeHtml,
  guessMimeByExt,
  safeImageSrc,
  safeLinkHref,
  useFilePreview,
} from "../src/composables/useFilePreview";
import type { FileInfo } from "../src/types";

const { getFileContentMock } = vi.hoisted(() => ({
  getFileContentMock: vi.fn(
    async (
      _path?: string,
      _commit?: string,
      _opts?: { signal?: AbortSignal },
    ) => new Blob(["hello"]),
  ),
}));

vi.mock("../src/services/files.service", () => ({
  filesService: { getFileContent: getFileContentMock },
}));

function file(name: string): FileInfo {
  return {
    id: name,
    name,
    path: name,
    kind: "file",
    created_at: "2026-01-01T00:00:00.000Z",
  };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("detectPreviewKind", () => {
  it("maps extensions to preview kinds", () => {
    expect(detectPreviewKind("photos/a.png")).toBe("image");
    expect(detectPreviewKind("notes/readme.md")).toBe("markdown");
    expect(detectPreviewKind("src/main.ts")).toBe("code");
    expect(detectPreviewKind("docs/manual.pdf")).toBe("pdf");
    expect(detectPreviewKind("media/clip.mp4")).toBe("video");
    expect(detectPreviewKind("media/song.mp3")).toBe("audio");
    expect(detectPreviewKind("notes/plain.txt")).toBe("text");
    expect(detectPreviewKind("LICENSE")).toBe("text");
    expect(detectPreviewKind("archive.bin")).toBe("unsupported");
  });
});

describe("guessMimeByExt", () => {
  it("returns a concrete mime type for known media", () => {
    expect(guessMimeByExt("a.png")).toBe("image/png");
    expect(guessMimeByExt("a.JPG")).toBe("image/jpeg");
    expect(guessMimeByExt("a.svg")).toBe("image/svg+xml");
    expect(guessMimeByExt("a.mp4")).toBe("video/mp4");
    expect(guessMimeByExt("a.flac")).toBe("audio/flac");
    expect(guessMimeByExt("a.unknown")).toBe("application/octet-stream");
  });
});

describe("preview sanitizers", () => {
  it("escapes html-significant characters", () => {
    expect(escapeHtml(`<img src="x" onerror='y'>&`)).toBe(
      "&lt;img src=&quot;x&quot; onerror=&#39;y&#39;&gt;&amp;",
    );
  });

  it("only allows safe link targets", () => {
    expect(safeLinkHref("https://example.com")).toBe("https://example.com");
    expect(safeLinkHref("/docs/a")).toBe("/docs/a");
    expect(safeLinkHref("#section")).toBe("#section");
    expect(safeLinkHref("mailto:a@b.co")).toBe("mailto:a@b.co");
    expect(safeLinkHref("javascript:alert(1)")).toBe("#");
    expect(safeLinkHref("data:text/html,<script>1</script>")).toBe("#");
    expect(safeLinkHref("")).toBe("#");
  });

  it("only allows safe image sources", () => {
    expect(safeImageSrc("https://example.com/a.png")).toBe(
      "https://example.com/a.png",
    );
    expect(safeImageSrc("/assets/a.png")).toBe("/assets/a.png");
    expect(safeImageSrc("data:image/png;base64,AAAA")).toBe(
      "data:image/png;base64,AAAA",
    );
    expect(safeImageSrc("javascript:alert(1)")).toBe("");
    expect(safeImageSrc("data:text/html,<b>")).toBe("");
    expect(safeImageSrc("relative.png")).toBe("");
  });
});

describe("useFilePreview rendering", () => {
  it("highlights fenced code blocks inside markdown", async () => {
    getFileContentMock.mockResolvedValueOnce(
      new Blob(["# 标题\n\n```ts\nconst a: number = 1;\n```\n"]),
    );
    const preview = useFilePreview(ref(undefined));

    await preview.openPreview("notes/readme.md");
    await flush();
    await flush();

    expect(preview.preview.value.kind).toBe("markdown");
    expect(preview.preview.value.html).toContain(
      '<code class="hljs language-ts">',
    );
    expect(preview.preview.value.html).toContain("hljs-keyword");
  });

  it("leaves markdown without code blocks untouched", async () => {
    getFileContentMock.mockResolvedValueOnce(new Blob(["# 只有标题\n"]));
    const preview = useFilePreview(ref(undefined));

    await preview.openPreview("notes/plain.md");
    await flush();
    await flush();

    expect(preview.preview.value.html).toContain("<h1");
    expect(preview.preview.value.html).not.toContain("hljs");
  });

  it("highlights code files by their extension", async () => {
    getFileContentMock.mockResolvedValueOnce(
      new Blob(["interface A { b: string }\nconst x: A = { b: 'v' };\n"]),
    );
    const preview = useFilePreview(ref(undefined));

    await preview.openPreview("src/types.ts");
    await flush();
    await flush();

    expect(preview.preview.value.kind).toBe("code");
    expect(preview.preview.value.html).toContain("hljs-keyword");
    // 模板里已经有外层 <pre>，这里只放高亮后的片段
    expect(preview.preview.value.html).not.toContain("<pre>");
  });
});

describe("useFilePreview cancellation", () => {
  it("discards a response that arrives after a newer preview", async () => {
    const slow = deferred<Blob>();
    getFileContentMock.mockReturnValueOnce(slow.promise);
    getFileContentMock.mockResolvedValueOnce(new Blob(["second"]));
    const preview = useFilePreview(ref(undefined));

    const first = preview.openPreview("a.txt");
    await preview.openPreview("b.txt");

    expect(preview.preview.value.path).toBe("b.txt");
    expect(preview.preview.value.text).toBe("second");
    // 旧请求此时才返回，必须被丢弃
    slow.resolve(new Blob(["first"]));
    await first;

    expect(preview.preview.value.path).toBe("b.txt");
    expect(preview.preview.value.text).toBe("second");
    expect(preview.preview.value.loading).toBe(false);
  });

  it("aborts the in-flight request when another preview starts", async () => {
    const slow = deferred<Blob>();
    getFileContentMock.mockReturnValueOnce(slow.promise);
    getFileContentMock.mockResolvedValueOnce(new Blob(["second"]));
    const preview = useFilePreview(ref(undefined));

    // 前面的用例也会调用该 mock，这里记录本次调用的下标
    const callIndex = getFileContentMock.mock.calls.length;
    const first = preview.openPreview("a.txt");
    const firstSignal = getFileContentMock.mock.calls[callIndex][2]?.signal;
    expect(firstSignal).toBeDefined();
    expect(firstSignal?.aborted).toBe(false);

    await preview.openPreview("b.txt");
    expect(firstSignal?.aborted).toBe(true);

    slow.resolve(new Blob(["first"]));
    await first;
  });

  it("keeps the preview empty when it is closed while loading", async () => {
    const slow = deferred<Blob>();
    getFileContentMock.mockReturnValueOnce(slow.promise);
    const preview = useFilePreview(ref(undefined));

    const pending = preview.openPreview("a.txt");
    preview.closePreview();
    slow.resolve(new Blob(["late"]));
    await pending;

    expect(preview.preview.value.open).toBe(false);
    expect(preview.preview.value.text).toBe("");
    expect(preview.preview.value.error).toBeNull();
  });

  it("does not report an aborted load as a failure", async () => {
    const slow = deferred<Blob>();
    getFileContentMock.mockReturnValueOnce(slow.promise);
    getFileContentMock.mockResolvedValueOnce(new Blob(["second"]));
    const preview = useFilePreview(ref(undefined));

    const first = preview.openPreview("a.txt");
    await preview.openPreview("b.txt");

    slow.reject(new DOMException("Aborted", "AbortError"));
    await first;

    expect(preview.preview.value.error).toBeNull();
    expect(preview.preview.value.text).toBe("second");
  });
});

describe("useFilePreview navigation", () => {
  it("moves to the previous/next previewable file and respects the ends", async () => {
    const files = [file("a.txt"), file("b.txt"), file("c.txt")];
    const preview = useFilePreview(ref(undefined), {
      getPreviewableFiles: () => files,
    });

    await preview.openPreview("b.txt");
    await flush();
    expect(preview.preview.value.path).toBe("b.txt");
    expect(preview.previewIndex.value).toBe(1);
    expect(preview.previewTotal.value).toBe(3);
    expect(preview.canGoPrev.value).toBe(true);
    expect(preview.canGoNext.value).toBe(true);

    preview.nextPreview();
    await flush();
    expect(preview.preview.value.path).toBe("c.txt");
    expect(preview.canGoNext.value).toBe(false);

    preview.nextPreview();
    await flush();
    expect(preview.preview.value.path).toBe("c.txt");

    preview.prevPreview();
    await flush();
    expect(preview.preview.value.path).toBe("b.txt");
    expect(preview.canGoPrev.value).toBe(true);
  });

  it("cannot navigate outside a single-file list", async () => {
    const files = [file("only.txt")];
    const preview = useFilePreview(ref(undefined), {
      getPreviewableFiles: () => files,
    });

    await preview.openPreview("only.txt");
    await flush();
    expect(preview.previewTotal.value).toBe(1);
    expect(preview.canGoPrev.value).toBe(false);
    expect(preview.canGoNext.value).toBe(false);
  });
});
