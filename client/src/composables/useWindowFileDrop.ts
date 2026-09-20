import { onBeforeUnmount, onMounted, ref, type Ref } from "vue";

export interface WindowFileDropOptions {
  /** 松开鼠标后拿到的文件列表（仅外部文件，内部拖拽不会触发）。 */
  onFiles: (files: File[]) => void;
  /** 是否响应拖放（例如已有弹窗时交给弹窗自己处理）。 */
  enabled?: () => boolean;
}

/** dataTransfer.types 里是否包含 Files：只有外部文件拖入才为真。 */
function hasFiles(event: DragEvent): boolean {
  const types = event.dataTransfer?.types;
  if (!types) return false;
  return Array.from(types).includes("Files");
}

/**
 * 整窗文件拖放。
 *
 * 主流网盘都支持把桌面文件直接拖进浏览器：这里在 window 上监听拖放事件，
 * 仅当拖动内容包含外部文件时激活，内部「拖动移动到文件夹」不受影响。
 * 拖入过程中返回的 `dragging` 用于渲染整窗提示浮层。
 */
export function useWindowFileDrop(options: WindowFileDropOptions): {
  dragging: Ref<boolean>;
} {
  const dragging = ref(false);
  // dragenter/dragleave 会在子元素间反复触发，用深度计数避免浮层闪烁
  let depth = 0;

  const isEnabled = () => options.enabled?.() ?? true;

  function onDragEnter(event: DragEvent) {
    if (!hasFiles(event) || !isEnabled()) return;
    depth += 1;
    dragging.value = true;
  }

  function onDragOver(event: DragEvent) {
    if (!hasFiles(event) || !isEnabled()) return;
    // 必须阻止默认行为，否则浏览器不会派发 drop（而是直接打开文件）
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
    dragging.value = true;
  }

  function onDragLeave(event: DragEvent) {
    if (!hasFiles(event)) return;
    depth = Math.max(0, depth - 1);
    if (depth === 0) dragging.value = false;
  }

  function onDrop(event: DragEvent) {
    if (!hasFiles(event)) return;
    // 阻止浏览器直接打开文件
    event.preventDefault();
    depth = 0;
    dragging.value = false;

    if (!isEnabled()) return;
    const files = Array.from(event.dataTransfer?.files ?? []);
    if (files.length > 0) options.onFiles(files);
  }

  onMounted(() => {
    window.addEventListener("dragenter", onDragEnter);
    window.addEventListener("dragover", onDragOver);
    window.addEventListener("dragleave", onDragLeave);
    window.addEventListener("drop", onDrop);
  });

  onBeforeUnmount(() => {
    window.removeEventListener("dragenter", onDragEnter);
    window.removeEventListener("dragover", onDragOver);
    window.removeEventListener("dragleave", onDragLeave);
    window.removeEventListener("drop", onDrop);
  });

  return { dragging };
}
