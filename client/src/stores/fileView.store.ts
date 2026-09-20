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
}

const SORT_FIELDS: SortField[] = ["name", "size", "modified", "type"];
const VIEW_MODES: FileViewMode[] = ["list", "grid"];
const MIN_THUMBNAIL_SIZE = 96;
const MAX_THUMBNAIL_SIZE = 240;
const DEFAULT_THUMBNAIL_SIZE = 144;

function clampThumbnailSize(value: unknown): number {
  const num = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(num)) return DEFAULT_THUMBNAIL_SIZE;
  return Math.min(
    MAX_THUMBNAIL_SIZE,
    Math.max(MIN_THUMBNAIL_SIZE, Math.round(num)),
  );
}

function readPersisted(): PersistedViewPrefs {
  const fallback: PersistedViewPrefs = {
    mode: "list",
    sortField: DEFAULT_SORT_STATE.field,
    sortDirection: DEFAULT_SORT_STATE.direction,
    foldersFirst: DEFAULT_SORT_STATE.foldersFirst,
    thumbnailSize: DEFAULT_THUMBNAIL_SIZE,
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

  function persist() {
    if (typeof localStorage === "undefined") return;
    try {
      const payload: PersistedViewPrefs = {
        mode: mode.value,
        sortField: sortField.value,
        sortDirection: sortDirection.value,
        foldersFirst: foldersFirst.value,
        thumbnailSize: thumbnailSize.value,
      };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
    } catch {
      // 存储不可用（隐私模式/配额）时静默降级，不影响浏览
    }
  }

  watch(
    [mode, sortField, sortDirection, foldersFirst, thumbnailSize],
    persist,
    {
      flush: "post",
    },
  );

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

  return {
    mode,
    sortField,
    sortDirection,
    foldersFirst,
    thumbnailSize,
    setMode,
    toggleMode,
    setSortField,
    setSortDirection,
    toggleSortDirection,
    toggleFoldersFirst,
    setThumbnailSize,
  };
});
