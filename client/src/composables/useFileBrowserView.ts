import { computed, ref } from "vue";
import { useFileViewStore } from "../stores/fileView.store";
import { useMediaQuery } from "./useMediaQuery";
import type { SortField, SortState } from "../utils/fileSort";

/**
 * 文件浏览器的视图层状态：移动端断点、列表/网格、排序与根视图菜单。
 *
 * 这些状态同时驱动工具栏、内容区与排序派生，集中在一处避免主组件里
 * 手写 matchMedia 监听和排序切换。
 */
export function useFileBrowserView() {
  const fileView = useFileViewStore();
  const isMobile = useMediaQuery("(max-width: 1023px)");
  const viewMenuOpen = ref(false);

  const sortState = computed<SortState>(() => ({
    field: fileView.sortField,
    direction: fileView.sortDirection,
    foldersFirst: fileView.foldersFirst,
  }));
  const viewMode = computed(() => fileView.mode);

  function handleSortChange(field: SortField) {
    if (fileView.sortField === field) {
      fileView.toggleSortDirection();
      return;
    }
    fileView.setSortField(field);
  }

  return {
    isMobile,
    viewMenuOpen,
    sortState,
    viewMode,
    handleSortChange,
  };
}
