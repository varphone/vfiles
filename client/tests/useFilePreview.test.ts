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
  getFileContentMock: vi.fn(async () => new Blob(["hello"])),
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
