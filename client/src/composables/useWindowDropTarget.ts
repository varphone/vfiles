import { computed, type ComputedRef, type Ref } from "vue";
import { useWindowFileDrop } from "./useWindowFileDrop";

interface WindowDropTargetDeps {
  previewOpen: Ref<boolean>;
  showUploader: Ref<boolean>;
  showHistory: Ref<boolean>;
  showShareDialog: Ref<boolean>;
  showMoveDialog: Ref<boolean>;
  dirManagerOpen: Ref<boolean>;
  showDetailsDialog: Ref<boolean>;
  canWrite: ComputedRef<boolean>;
  currentPath: Ref<string>;
  queueDroppedFiles: (files: File[]) => void;
}

/**
 * 整窗拖放上传的目标状态：有弹窗时不抢拖放，目标名取当前目录末级。
 */
export function useWindowDropTarget(deps: WindowDropTargetDeps) {
  /** 已有弹窗（预览/上传/移动/历史/分享）时交给弹窗自己处理。 */
  const anyDialogOpen = computed(
    () =>
      deps.previewOpen.value ||
      deps.showUploader.value ||
      deps.showHistory.value ||
      deps.showShareDialog.value ||
      deps.showMoveDialog.value ||
      deps.dirManagerOpen.value ||
      deps.showDetailsDialog.value,
  );

  const dropTargetLabel = computed(() =>
    deps.currentPath.value
      ? deps.currentPath.value.split("/").filter(Boolean).pop() ||
        deps.currentPath.value
      : "根目录",
  );

  const { dragging: externalDropActive } = useWindowFileDrop({
    onFiles: (files) => deps.queueDroppedFiles(files),
    enabled: () => !anyDialogOpen.value && deps.canWrite.value,
  });

  return { anyDialogOpen, dropTargetLabel, externalDropActive };
}
