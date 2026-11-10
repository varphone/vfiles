import { computed, onBeforeUnmount, ref, watch, type Ref } from "vue";
import type { FileInfo } from "../types";

interface DragChipDeps {
  canWrite: Ref<boolean>;
  moveEntryToDirectory: (file: FileInfo, targetDir: string) => Promise<boolean>;
}

/**
 * 光标跟随的拖拽 chip（VS Code/macOS 风格）：显示条目名/多选数、目标目录，
 * 以及非法落点的禁止态。位置由捕获阶段的 dragover 实时更新。
 */
export function useDragChip(
  selectedPaths: Ref<Set<string>>,
  deps: DragChipDeps,
) {
  const draggingFile = ref<FileInfo | null>(null);
  const dragChipPos = ref({ x: -999, y: -999 });
  const dragChipTarget = ref<string | null>(null);
  const dragChipInvalid = ref(false);
  const dropFlashPath = ref("");

  const dragChipLabel = computed(() => {
    const file = draggingFile.value;
    if (!file) return "";
    if (dragChipInvalid.value) return "不能放到这里";
    if (dragChipTarget.value) return `放入 ${dragChipTarget.value}`;
    const selected = selectedPaths.value;
    if (
      selected instanceof Set &&
      selected.size > 1 &&
      selected.has(file.path)
    ) {
      return `移动 ${selected.size} 项`;
    }
    return `移动 ${file.name}`;
  });

  function moveDragChip(event: DragEvent) {
    dragChipPos.value = { x: event.clientX + 14, y: event.clientY + 14 };
    // 两段式：命中有效落点（树条目 / 目录行）→「放入 目标名」
    const element = document.elementFromPoint(
      event.clientX,
      event.clientY,
    );
    const tree = element?.closest(".directory-tree-item") as HTMLElement | null;
    const row = element?.closest("tr[data-vfiles-path]") as HTMLElement | null;
    const rowIsDir = row?.querySelector("a.desktop-name-link") != null;
    const target = tree ?? (rowIsDir ? row : null);
    const targetPath =
      target?.getAttribute("data-vfiles-path") ??
      row?.getAttribute("data-vfiles-path") ??
      "";
    const draggedPath = draggingFile.value?.path ?? "";

    // 拖自身任何行 = 禁止（先于目录目标判定）
    if (
      row &&
      draggedPath &&
      row.getAttribute("data-vfiles-path") === draggedPath
    ) {
      dragChipTarget.value = null;
      dragChipInvalid.value = true;
      return;
    }
    // 非法目标（拖自身 / 自身子目录）→ 禁止态
    if (
      target &&
      draggedPath &&
      (targetPath === draggedPath || targetPath.startsWith(`${draggedPath}/`))
    ) {
      dragChipTarget.value = null;
      dragChipInvalid.value = true;
      return;
    }
    dragChipInvalid.value = false;
    if (!target) {
      dragChipTarget.value = null;
      return;
    }
    dragChipTarget.value =
      (
        target.querySelector(".desktop-name-text, [class*=tree-name]")
          ?.textContent ??
        target.textContent ??
        ""
      )
        .trim()
        .split("\n")[0]
        .slice(0, 24) || null;
  }

  async function handleDropOnFolder(targetDir: string) {
    if (!deps.canWrite.value) return;
    const file = draggingFile.value;
    draggingFile.value = null;
    if (!file) return;
    // 落子回执：目标行 650ms 高亮消退
    dropFlashPath.value = targetDir;
    window.setTimeout(() => {
      if (dropFlashPath.value === targetDir) dropFlashPath.value = "";
    }, 650);
    // 自身/子目录/未变化/重名由 useMoveDialog 的 planMoveOperations 统一校验
    await deps.moveEntryToDirectory(file, targetDir);
  }

  watch(draggingFile, (current) => {
    if (current) {
      document.addEventListener("dragover", moveDragChip, {
        passive: true,
        capture: true,
      });
    } else {
      document.removeEventListener("dragover", moveDragChip, {
        capture: true,
      });
      dragChipPos.value = { x: -999, y: -999 };
      dragChipTarget.value = null;
      dragChipInvalid.value = false;
    }
  });

  onBeforeUnmount(() => {
    document.removeEventListener("dragover", moveDragChip, { capture: true });
  });

  return {
    draggingFile,
    dragChipPos,
    dragChipTarget,
    dragChipInvalid,
    dragChipLabel,
    dropFlashPath,
    handleDropOnFolder,
    handleDragStart: (file: FileInfo) => {
      draggingFile.value = file;
    },
    handleDragEnd: () => {
      draggingFile.value = null;
    },
  };
}
