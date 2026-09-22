import { defineStore } from "pinia";
import { ref, watch } from "vue";
import {
  DEFAULT_SORT_STATE,
  type SortDirection,
  type SortField,
} from "../utils/fileSort";

export type FileViewMode = "list" | "grid";

const STORAGE_KEY = "vfiles:file-browser:view";

interface PersistedViewPrefs {
  mode: FileViewMode;
  sortField: SortField;
  sortDirection: SortDirection;
  foldersFirst: boolean;
  thumbnailSize: number;
  /** 桌面端是否显示右侧「详细信息」面板 */
  detailsVisible: boolean;
  /** 桌面列表列宽（像素） */
  columnWidths: Partial<Record<ColumnWidthKey, number>>;
}

const SORT_FIELDS: SortField[] = ["name", "size", "modified", "type"];
const VIEW_MODES: FileViewMode[] = ["list", "grid"];
const MIN_THUMBNAIL_SIZE = 96;
const MAX_THUMBNAIL_SIZE = 240;
const DEFAULT_THUMBNAIL_SIZE = 144;

/** 网格卡片最小列宽（--file-card-min）：网格与骨架屏共用，保证加载态几何一致。 */
export function cardMinWidth(thumbnailSize: number): number {
  return Math.round(thumbnailSize * 1.35);
}

function clampThumbnailSize(value: unknown): number {
  const num = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(num)) return DEFAULT_THUMBNAIL_SIZE;
  return Math.min(
    MAX_THUMBNAIL_SIZE,
    Math.max(MIN_THUMBNAIL_SIZE, Math.round(num)),
  );
}

/** 桌面列表可拖拽的列（勾选框与操作列不参与）。 */
export type ColumnWidthKey = "name" | "modified" | "type" | "size";

export const DEFAULT_COLUMN_WIDTHS: Record<ColumnWidthKey, number> = {
  name: 320,
  modified: 150,
  type: 130,
  size: 96,
};

export const MIN_COLUMN_WIDTH = 88;
export const MAX_COLUMN_WIDTH = 640;

const COLUMN_KEYS: ColumnWidthKey[] = ["name", "modified", "type", "size"];

function clampColumnWidth(value: unknown, fallback: number): number {
  const num = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(num)) return fallback;
  return Math.min(
    MAX_COLUMN_WIDTH,
    Math.max(MIN_COLUMN_WIDTH, Math.round(num)),
  );
}

function readPersistedColumnWidths(
  raw: Partial<Record<ColumnWidthKey, unknown>> | undefined,
): Partial<Record<ColumnWidthKey, number>> {
  const widths = { ...DEFAULT_COLUMN_WIDTHS };
  if (!raw) return widths;
  for (const key of COLUMN_KEYS) {
    widths[key] = clampColumnWidth(raw[key], DEFAULT_COLUMN_WIDTHS[key]);
  }
  return widths;
}

function readPersisted(): PersistedViewPrefs {
  const fallback: PersistedViewPrefs = {
    mode: "list",
    sortField: DEFAULT_SORT_STATE.field,
    sortDirection: DEFAULT_SORT_STATE.direction,
    foldersFirst: DEFAULT_SORT_STATE.foldersFirst,
    thumbnailSize: DEFAULT_THUMBNAIL_SIZE,
    detailsVisible: true,
    columnWidths: {},
  };

  if (typeof localStorage === "undefined") return fallback;

  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return fallback;
    const parsed = JSON.parse(raw) as Partial<PersistedViewPrefs>;
    return {
      mode: VIEW_MODES.includes(parsed.mode as FileViewMode)
        ? (parsed.mode as FileViewMode)
        : fallback.mode,
      sortField: SORT_FIELDS.includes(parsed.sortField as SortField)
        ? (parsed.sortField as SortField)
        : fallback.sortField,
      sortDirection:
        parsed.sortDirection === "desc" ? "desc" : fallback.sortDirection,
      foldersFirst:
        typeof parsed.foldersFirst === "boolean"
          ? parsed.foldersFirst
          : fallback.foldersFirst,
      thumbnailSize: clampThumbnailSize(parsed.thumbnailSize),
      detailsVisible:
        typeof parsed.detailsVisible === "boolean"
          ? parsed.detailsVisible
          : fallback.detailsVisible,
      columnWidths: readPersistedColumnWidths(parsed.columnWidths),
    };
  } catch {
    return fallback;
  }
}

export const useFileViewStore = defineStore("fileView", () => {
  const initial = readPersisted();

  const mode = ref<FileViewMode>(initial.mode);
  const sortField = ref<SortField>(initial.sortField);
  const sortDirection = ref<SortDirection>(initial.sortDirection);
  const foldersFirst = ref(initial.foldersFirst);
  const thumbnailSize = ref(initial.thumbnailSize);
  const detailsVisible = ref(initial.detailsVisible);
  /** 桌面列表列宽（可拖拽，随视图偏好一起持久化）。 */
  const columnWidths = ref<Partial<Record<ColumnWidthKey, number>>>({
    ...initial.columnWidths,
  });

  function persist() {
    if (typeof localStorage === "undefined") return;
    try {
      const payload: PersistedViewPrefs = {
        mode: mode.value,
        sortField: sortField.value,
        sortDirection: sortDirection.value,
        foldersFirst: foldersFirst.value,
        thumbnailSize: thumbnailSize.value,
        detailsVisible: detailsVisible.value,
        columnWidths: { ...columnWidths.value },
      };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
    } catch {
      // 存储不可用（隐私模式/配额）时静默降级，不影响浏览
    }
  }

  watch(
    [
      mode,
      sortField,
      sortDirection,
      foldersFirst,
      thumbnailSize,
      detailsVisible,
      columnWidths,
    ],
    persist,
    {
      flush: "post",
    },
  );

  /** 设置某一列宽度（自动夹在允许范围内）。 */
  function setColumnWidth(key: ColumnWidthKey, width: number) {
    if (!COLUMN_KEYS.includes(key)) return;
    const clamped = clampColumnWidth(width, DEFAULT_COLUMN_WIDTHS[key]);
    // 恢复默认 = 删除存储：名称列回到"自适应吸收余量"，其余列回落默认宽度。
    const next = { ...columnWidths.value };
    if (clamped === DEFAULT_COLUMN_WIDTHS[key]) {
      delete next[key];
    } else {
      next[key] = clamped;
    }
    columnWidths.value = next;
  }

  /** 双击列边界时恢复该列默认宽度。 */
  function resetColumnWidth(key: ColumnWidthKey) {
    setColumnWidth(key, DEFAULT_COLUMN_WIDTHS[key]);
  }

  function setMode(next: FileViewMode) {
    if (VIEW_MODES.includes(next)) mode.value = next;
  }

  function toggleMode() {
    mode.value = mode.value === "list" ? "grid" : "list";
  }

  function setSortField(next: SortField) {
    if (!SORT_FIELDS.includes(next)) return;
    sortField.value = next;
  }

  function setSortDirection(next: SortDirection) {
    sortDirection.value = next === "desc" ? "desc" : "asc";
  }

  function toggleSortDirection() {
    sortDirection.value = sortDirection.value === "asc" ? "desc" : "asc";
  }

  function toggleFoldersFirst() {
    foldersFirst.value = !foldersFirst.value;
  }

  function setThumbnailSize(next: number) {
    thumbnailSize.value = clampThumbnailSize(next);
  }

  /** 桌面端右侧「详细信息」面板开关（移动端不展示，见 FileBrowser）。 */
  function setDetailsVisible(next: boolean) {
    detailsVisible.value = Boolean(next);
  }

  function toggleDetails() {
    detailsVisible.value = !detailsVisible.value;
  }

  return {
    mode,
    sortField,
    sortDirection,
    foldersFirst,
    thumbnailSize,
    detailsVisible,
    columnWidths,
    setColumnWidth,
    resetColumnWidth,
    setDetailsVisible,
    toggleDetails,
    setMode,
    toggleMode,
    setSortField,
    setSortDirection,
    toggleSortDirection,
    toggleFoldersFirst,
    setThumbnailSize,
  };
});
