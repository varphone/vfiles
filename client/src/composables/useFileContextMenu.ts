import { computed, ref, type Ref } from "vue";
import {
  IconArrowsDiff,
  IconCheck,
  IconDownload,
  IconEye,
  IconFolderOpen,
  IconFolderPlus,
  IconHistory,
  IconInfoCircle,
  IconPencil,
  IconShare,
  IconStar,
  IconStarFilled,
  IconTrash,
  IconUserShare,
} from "@tabler/icons-vue";
import type { FileInfo } from "../types";
import { filesService } from "../services/files.service";
import type { ContextMenuItem } from "../components/file-browser/ContextMenu.vue";

interface FileContextMenuDeps {
  files: Ref<FileInfo[]>;
  canWrite: Ref<boolean>;
  canShare: Ref<boolean>;
  isRestricted: Ref<boolean>;
  batchMode: Ref<boolean>;
  selectedPaths: Ref<Set<string>>;
  desktopActivePath: Ref<string>;
  narrowSelectionTo: (path: string) => void;
  toggleSelect: (file: FileInfo) => void;
  openDetailsDialog: (file: FileInfo) => void;
  handleOpenFolder: (file: FileInfo) => void;
  handleCreateDirectory: (file: FileInfo) => void | Promise<void>;
  handlePreview: (file: FileInfo) => void;
  handleViewHistory: (file: FileInfo) => void;
  handleRenameEntry: (file: FileInfo) => void;
  handleMoveEntry: (file: FileInfo) => void;
  openTransferDialog: (files: FileInfo[]) => void;
  handleDownload: (file: FileInfo) => void;
  handleShare: (file: FileInfo) => void;
  handleDelete: (file: FileInfo) => void | Promise<void>;
  notifySuccess: (message: string) => void;
  notifyError: (message: string) => void;
}

/**
 * 右键/键盘行菜单与收藏状态。
 *
 * 菜单位置、键盘焦点归还、首项「选择」和收藏切换都集中在这里；FileBrowser
 * 只负责注入条目动作。
 */
export function useFileContextMenu(deps: FileContextMenuDeps) {
  const favoritePaths = ref<Set<string>>(new Set());
  const contextMenu = ref<{
    show: boolean;
    x: number;
    y: number;
    file: FileInfo | null;
    returnFocus: HTMLElement | null;
  }>({ show: false, x: 0, y: 0, file: null, returnFocus: null });

  function handleFavoriteChanged(path: string, isFavorite: boolean) {
    const next = new Set(favoritePaths.value);
    if (isFavorite) next.add(path);
    else next.delete(path);
    favoritePaths.value = next;
    for (const file of deps.files.value) {
      if (file.path === path) file.is_favorite = isFavorite;
    }
    if (contextMenu.value.file?.path === path) {
      contextMenu.value.file.is_favorite = isFavorite;
    }
  }

  function isFavorite(file: FileInfo): boolean {
    return file.is_favorite ?? favoritePaths.value.has(file.path);
  }

  async function toggleFavorite(file: FileInfo) {
    const wasFavorite = isFavorite(file);
    try {
      if (wasFavorite) await filesService.removeFavorite(file.path);
      else await filesService.addFavorite(file.path);
      file.is_favorite = !wasFavorite;
      handleFavoriteChanged(file.path, !wasFavorite);
      deps.notifySuccess(
        wasFavorite ? `已取消收藏「${file.name}」` : `已收藏「${file.name}」`,
      );
    } catch (err) {
      deps.notifyError(err instanceof Error ? err.message : "收藏操作失败");
    }
  }

  const contextMenuItems = computed<ContextMenuItem[]>(() => {
    const file = contextMenu.value.file;
    if (!file) return [];
    const isDirectory = file.kind === "directory";
    const items: ContextMenuItem[] = [];

    if (deps.canWrite.value) {
      items.push({ key: "select", label: "选择", icon: IconCheck });
    }
    if (isDirectory) {
      items.push({
        key: "open",
        label: "打开",
        icon: IconFolderOpen,
        shortcut: "Enter",
      });
      if (deps.canWrite.value) {
        items.push({
          key: "create-directory",
          label: "在此新建子目录",
          icon: IconFolderPlus,
        });
      }
    } else {
      items.push({ key: "preview", label: "预览", icon: IconEye });
      items.push({ key: "history", label: "历史版本", icon: IconHistory });
    }
    items.push({ key: "details", label: "详细信息", icon: IconInfoCircle });
    if (deps.canWrite.value) {
      const favorite = isFavorite(file);
      items.push({
        key: "favorite",
        label: favorite ? "取消收藏" : "加入收藏",
        icon: favorite ? IconStarFilled : IconStar,
      });
      items.push({
        key: "rename",
        label: "重命名",
        icon: IconPencil,
        shortcut: "F2",
      });
      items.push({ key: "move", label: "移动", icon: IconArrowsDiff });
    }
    if (!deps.isRestricted.value) {
      items.push({
        key: "transfer",
        label: "转移所有权",
        icon: IconUserShare,
      });
    }
    items.push({ key: "download", label: "下载", icon: IconDownload });
    if (deps.canShare.value) {
      items.push({ key: "share", label: "分享", icon: IconShare });
    }
    if (deps.canWrite.value) {
      items.push({
        key: "delete",
        label: "删除",
        icon: IconTrash,
        danger: true,
        shortcut: "Del",
      });
    }
    return items;
  });

  function openRowContextMenu(file: FileInfo) {
    if (typeof document === "undefined") return;
    const selector = `[data-vfiles-path="${CSS.escape(file.path)}"]`;
    const element = document.querySelector<HTMLElement>(selector);
    const rect = element?.getBoundingClientRect();
    const returnFocus =
      element?.querySelector<HTMLElement>(".desktop-row-menu") ?? null;
    handleContextMenu({
      file,
      x: rect ? rect.right - 8 : 0,
      y: rect ? rect.bottom : 0,
      returnFocus,
    });
  }

  function handleContextMenu(payload: {
    file: FileInfo;
    x: number;
    y: number;
    returnFocus?: HTMLElement | null;
  }) {
    deps.narrowSelectionTo(payload.file.path);
    deps.desktopActivePath.value = payload.file.path;
    contextMenu.value = {
      show: true,
      x: payload.x,
      y: payload.y,
      file: payload.file,
      returnFocus: payload.returnFocus ?? null,
    };
  }

  /** 关闭右键菜单：键盘打开时把焦点还给触发行，鼠标操作保持原焦点。 */
  function closeContextMenu() {
    const target = contextMenu.value.returnFocus;
    contextMenu.value.show = false;
    contextMenu.value.returnFocus = null;
    if (target?.isConnected) target.focus();
  }

  function handleContextMenuSelect(key: string) {
    const file = contextMenu.value.file;
    if (!file) return;
    switch (key) {
      case "select":
        if (!deps.canWrite.value) return;
        deps.batchMode.value = true;
        if (!deps.selectedPaths.value.has(file.path)) deps.toggleSelect(file);
        break;
      case "details":
        deps.openDetailsDialog(file);
        break;
      case "favorite":
        if (!deps.canWrite.value) return;
        void toggleFavorite(file);
        break;
      case "open":
        deps.handleOpenFolder(file);
        break;
      case "create-directory":
        void deps.handleCreateDirectory(file);
        break;
      case "preview":
        deps.handlePreview(file);
        break;
      case "history":
        deps.handleViewHistory(file);
        break;
      case "rename":
        if (!deps.canWrite.value) return;
        void deps.handleRenameEntry(file);
        break;
      case "move":
        if (!deps.canWrite.value) return;
        deps.handleMoveEntry(file);
        break;
      case "transfer":
        if (deps.isRestricted.value) return;
        deps.openTransferDialog([file]);
        break;
      case "download":
        deps.handleDownload(file);
        break;
      case "share":
        if (!deps.canShare.value) return;
        deps.handleShare(file);
        break;
      case "delete":
        if (!deps.canWrite.value) return;
        void deps.handleDelete(file);
        break;
    }
  }

  return {
    contextMenu,
    contextMenuItems,
    openRowContextMenu,
    handleContextMenu,
    closeContextMenu,
    handleContextMenuSelect,
  };
}
