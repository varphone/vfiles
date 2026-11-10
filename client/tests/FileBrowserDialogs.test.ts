import { fireEvent, render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import FileBrowserDialogs from "../src/components/file-browser/FileBrowserDialogs.vue";
import type { FileInfo } from "../src/types";
import type { PreviewState } from "../src/composables/useFilePreview";

const file: FileInfo = {
  id: "file-1",
  name: "report.md",
  path: "docs/report.md",
  kind: "file",
  size_bytes: 128,
  created_at: "2026-05-29T00:00:00.000Z",
};

const preview: PreviewState = {
  open: false,
  loading: false,
  error: null,
  path: "",
  kind: "text",
  text: "",
  html: "",
  objectUrl: "",
};

function baseProps() {
  return {
    showHistory: false,
    showShare: false,
    showDetails: false,
    showTransfer: false,
    showShortcuts: false,
    selectedFile: null,
    detailsFile: null,
    transferItems: [],
    canWrite: true,
    preview,
    previewFilename: "",
    canGoPrev: false,
    canGoNext: false,
    previewPosition: 0,
    previewTotal: 0,
    canCopyPreview: false,
    copyState: "idle" as const,
    previewFile: null,
  };
}

describe("FileBrowserDialogs.vue", () => {
  it("emits the details actions with the selected file", async () => {
    const { emitted } = render(FileBrowserDialogs, {
      props: { ...baseProps(), showDetails: true, detailsFile: file },
    });

    await fireEvent.click(screen.getByRole("button", { name: "预览" }));
    await fireEvent.click(screen.getByRole("button", { name: "下载" }));
    await fireEvent.click(screen.getByRole("button", { name: "分享" }));

    expect(emitted()["details-preview"]?.[0]).toEqual([file]);
    expect(emitted()["details-download"]?.[0]).toEqual([file]);
    expect(emitted()["details-share"]?.[0]).toEqual([file]);
  });

});
