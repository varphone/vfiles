import { onBeforeUnmount, onMounted, type Ref } from "vue";
import {
  createFileBrowserKeyboard,
  type FileBrowserKeyboardDeps,
} from "./useFileBrowserKeyboard";

export interface FileBrowserHotkeyDeps extends FileBrowserKeyboardDeps {
  desktopSearchBoxRef: Ref<HTMLElement | null>;
}

/**
 * 文件浏览器的文档级事件编排：全局键盘、点击/触摸外部关闭桌面搜索。
 *
 * 键盘语义本身在 createFileBrowserKeyboard 中；这里只管挂载/卸载与外部点击。
 */
export function useFileBrowserHotkeys(deps: FileBrowserHotkeyDeps) {
  onMounted(() => {
    const onDocPointer = (event: MouseEvent | TouchEvent) => {
      const target = event.target as Node | null;
      if (!target) return;
      if (!deps.desktopSearchOpen.value) return;
      const searchBox = deps.desktopSearchBoxRef.value;
      if (searchBox && !searchBox.contains(target)) deps.closeDesktopSearch();
    };

    const onDocKeydown = createFileBrowserKeyboard(deps);

    document.addEventListener("click", onDocPointer, true);
    document.addEventListener("touchstart", onDocPointer, true);
    document.addEventListener("keydown", onDocKeydown);
    onBeforeUnmount(() => {
      document.removeEventListener("click", onDocPointer, true);
      document.removeEventListener("touchstart", onDocPointer, true);
      document.removeEventListener("keydown", onDocKeydown);
    });
  });
}
