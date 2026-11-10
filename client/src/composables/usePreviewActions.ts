import { computed, ref, watch, type Ref } from "vue";
import type { FileInfo } from "../types";
import type { PreviewState } from "./useFilePreview";
import { copyText } from "../utils/clipboard";

interface PreviewActionDeps {
  preview: Ref<PreviewState>;
  files: Ref<FileInfo[]>;
  searchResults: Ref<FileInfo[]>;
  closePreview: () => void;
  searchActive: Ref<boolean>;
  clearSearch: () => void;
  loadFiles: (path: string) => Promise<void>;
  setActivePath: (path: string) => void;
  handleDownload: (file: FileInfo) => void;
}

/**
 * 预览顶栏/详情动作：复制原文、下载、在文件夹中显示，以及当前预览条目。
 *
 * 预览加载本身由 useFilePreview 负责；这里只处理围绕预览的 UI 动作。
 */
export function usePreviewActions(deps: PreviewActionDeps) {
  const previewCopyState = ref<"idle" | "done" | "failed">("idle");
  const canCopyPreview = computed(
    () =>
      (deps.preview.value.kind === "code" ||
        deps.preview.value.kind === "text") &&
      !deps.preview.value.loading &&
      !deps.preview.value.error &&
      deps.preview.value.text.length > 0,
  );

  /** 当前预览的条目：用于顶栏图标、大小与「下载 / 所在文件夹」入口。 */
  const previewFile = computed<FileInfo | null>(() => {
    const path = deps.preview.value.path;
    if (!path) return null;
    return (
      [...deps.files.value, ...deps.searchResults.value].find(
        (item) => item.path === path,
      ) ?? null
    );
  });

  function downloadPreviewedFile() {
    const file = previewFile.value;
    if (file) deps.handleDownload(file);
  }

  /** 打开所在文件夹并高亮该文件（主流预览器的「在文件夹中显示」）。 */
  async function revealPreviewedFile() {
    const file = previewFile.value;
    if (!file) return;
    const slash = file.path.lastIndexOf("/");
    const parent = slash > 0 ? file.path.slice(0, slash) : "";

    deps.closePreview();
    if (deps.searchActive.value) deps.clearSearch();
    await deps.loadFiles(parent);
    deps.setActivePath(file.path);
  }

  async function copyPreviewContent() {
    const ok = await copyText(deps.preview.value.text);
    previewCopyState.value = ok ? "done" : "failed";
    window.setTimeout(() => {
      previewCopyState.value = "idle";
    }, 2000);
  }

  watch(
    () => deps.preview.value.path,
    () => {
      previewCopyState.value = "idle";
    },
  );

  return {
    previewCopyState,
    canCopyPreview,
    previewFile,
    downloadPreviewedFile,
    revealPreviewedFile,
    copyPreviewContent,
  };
}
