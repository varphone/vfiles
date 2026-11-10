import type { Ref } from "vue";
import type { FileInfo } from "../types";

export interface FileBrowserKeyboardDeps {
  viewMenuOpen: Ref<boolean>;
  typeAheadPrefix: Ref<string>;
  clearTypeAhead: () => void;
  desktopSearchOpen: Ref<boolean>;
  closeDesktopSearch: () => void;
  previewOpen: Ref<boolean>;
  closePreview: () => void;
  prevPreview: () => void;
  nextPreview: () => void;
  showShortcuts: Ref<boolean>;
  batchMode: Ref<boolean>;
  toggleBatchMode: () => void;
  selectedPaths: Ref<Set<string>>;
  clearSelection: () => void;
  canWrite: Ref<boolean>;
  selectAllVisible: () => void;
  batchDelete: () => void | Promise<void>;
  findActiveItem: () => FileInfo | undefined;
  handleDelete: (file: FileInfo) => void | Promise<void>;
  moveActiveRow: (key: string, shift: boolean) => void;
  toggleActiveRowSelection: () => void;
  quickLook: () => void;
  handleTypeAhead: (character: string) => boolean;
  openRowContextMenu: (file: FileInfo) => void;
  renameSelected: () => void | Promise<void>;
  handleRenameEntry: (file: FileInfo) => void | Promise<void>;
  handleOpenFolder: (file: FileInfo) => void;
  handlePreview: (file: FileInfo) => void;
  anyOverlayOpen: () => boolean;
}

const OVERLAY_INPUT_TAGS = new Set(["INPUT", "TEXTAREA", "SELECT"]);

export function isTypingTarget(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element) return false;
  if (OVERLAY_INPUT_TAGS.has(element.tagName)) return true;
  return element.isContentEditable === true;
}

/** 可点击/可激活的控件：这些元素上的按键必须交还浏览器。 */
const INTERACTIVE_SELECTOR =
  'a[href], button, input, select, textarea, summary, [role="button"], [role="menuitem"], [role="menuitemradio"], [role="tab"], [contenteditable="true"]';

export function isInteractiveTarget(target: EventTarget | null): boolean {
  const element = target instanceof Element ? target : null;
  return element !== null && element.closest(INTERACTIVE_SELECTOR) !== null;
}

/**
 * 文件浏览器的全局键盘处理。
 *
 * 和组件解耦后，Escape 分层、控件让位、列表导航、预览/批量快捷键都集中
 * 在这里；组件只负责把状态与动作注入进来。
 */
export function createFileBrowserKeyboard(
  deps: FileBrowserKeyboardDeps,
): (event: KeyboardEvent) => void {
  const {
    viewMenuOpen,
    typeAheadPrefix,
    clearTypeAhead,
    desktopSearchOpen,
    closeDesktopSearch,
    previewOpen,
    closePreview,
    prevPreview,
    nextPreview,
    showShortcuts,
    batchMode,
    toggleBatchMode,
    selectedPaths,
    clearSelection,
    canWrite,
    selectAllVisible,
    batchDelete,
    findActiveItem,
    handleDelete,
    moveActiveRow,
    toggleActiveRowSelection,
    quickLook,
    handleTypeAhead,
    openRowContextMenu,
    renameSelected,
    handleRenameEntry,
    handleOpenFolder,
    handlePreview,
    anyOverlayOpen,
  } = deps;

  return (e: KeyboardEvent) => {

    if (e.defaultPrevented) return;

    // Escape 逐层退出：视图菜单 → 按键定位前缀 → 高级搜索 → 预览 → 批量模式 → 选择
    if (e.key === "Escape") {
      if (viewMenuOpen.value) {
        e.preventDefault();
        viewMenuOpen.value = false;
        return;
      }
      if (typeAheadPrefix.value) {
        e.preventDefault();
        clearTypeAhead();
        return;
      }
      if (desktopSearchOpen.value) {
        closeDesktopSearch();
        return;
      }
      if (previewOpen.value) {
        closePreview();
        return;
      }
      if (batchMode.value) {
        toggleBatchMode();
        return;
      }
      if (selectedPaths.value.size > 0) clearSelection();
      return;
    }

    // 预览打开时用左右方向键切换上一个/下一个
    if (previewOpen.value) {
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        prevPreview();
        return;
      }
      if (e.key === "ArrowRight") {
        e.preventDefault();
        nextPreview();
        return;
      }
    }

    // ? 打开/关闭快捷键面板：打开时再按一次即可关闭（此时焦点可能在面板内）
    if (e.key === "?") {
      if (showShortcuts.value) {
        e.preventDefault();
        showShortcuts.value = false;
        return;
      }
      if (!isTypingTarget(e.target)) {
        e.preventDefault();
        showShortcuts.value = true;
        return;
      }
    }

    // 焦点在按钮、链接、菜单项等交互控件上时，Enter/Space/方向键属于控件本身：
    // 全局快捷键若继续 preventDefault，键盘用户就无法激活工具栏/侧栏/菜单
    //（WCAG 2.1.1）。文件列表快捷键只在焦点不在控件上时生效。
    if (isInteractiveTarget(e.target)) return;

    // 输入控件或弹窗内不触发文件操作快捷键
    if (anyOverlayOpen() || isTypingTarget(e.target)) return;

    const modifier = e.ctrlKey || e.metaKey;
    if (modifier && (e.key === "a" || e.key === "A")) {
      e.preventDefault();
      if (!canWrite.value) return;
      if (!batchMode.value) batchMode.value = true;
      selectAllVisible();
      return;
    }

    if (e.key === "Delete" || e.key === "Backspace") {
      if (!canWrite.value) return;
      if (selectedPaths.value.size > 0) {
        e.preventDefault();
        void batchDelete();
        return;
      }
      const active = findActiveItem();
      if (active) {
        e.preventDefault();
        void handleDelete(active);
      }
      return;
    }

    if (
      e.key === "ArrowUp" ||
      e.key === "ArrowDown" ||
      e.key === "ArrowLeft" ||
      e.key === "ArrowRight" ||
      e.key === "Home" ||
      e.key === "End" ||
      e.key === "PageUp" ||
      e.key === "PageDown"
    ) {
      e.preventDefault();
      moveActiveRow(e.key, e.shiftKey);
      return;
    }

    // Space：快速预览（云盘 web 惯例 ✓ Drive/OneDrive/Dropbox 同款；Finder Quick Look）
    // Ctrl+Space：切换活动行选中态（原空格行为保留于此组合键）
    if (e.key === " " || e.key === "Spacebar") {
      e.preventDefault();
      if (e.ctrlKey || e.metaKey) {
        toggleActiveRowSelection();
      } else {
        quickLook();
      }
      return;
    }

    // 可打印字符（不含组合键）：按名称前缀定位；有匹配才拦截，否则放行
    if (
      !modifier &&
      !e.altKey &&
      e.key.length === 1 &&
      e.key !== " " &&
      /[\p{L}\p{N}._-]/u.test(e.key) &&
      handleTypeAhead(e.key)
    ) {
      e.preventDefault();
      return;
    }

    // 键盘打开行菜单（Windows 惯例：Shift+F10 或菜单键）
    if ((e.shiftKey && e.key === "F10") || e.key === "ContextMenu") {
      const target = findActiveItem();
      if (target) {
        e.preventDefault();
        openRowContextMenu(target);
        return;
      }
    }

    if (e.key === "F2") {
      if (!canWrite.value) return;
      if (selectedPaths.value.size === 1) {
        e.preventDefault();
        void renameSelected();
        return;
      }
      const active = findActiveItem();
      if (active) {
        e.preventDefault();
        void handleRenameEntry(active);
      }
      return;
    }

    if (e.key === "Enter") {
      const active = findActiveItem();
      if (!active) return;
      e.preventDefault();
      if (active.kind === "directory") {
        handleOpenFolder(active);
      } else {
        handlePreview(active);
      }
    }
    };
}
