/**
 * 文件列表排序工具。
 *
 * 主流云盘（Google Drive / OneDrive / 阿里云盘等）默认都提供
 * “名称 / 修改时间 / 大小 / 类型”排序与升序降序切换，并且目录始终排在文件之前。
 * 这里把该行为抽成纯函数，便于复用与单测。
 */
import type { FileInfo } from "../types";

export type SortField = "name" | "size" | "modified" | "type";
export type SortDirection = "asc" | "desc";

export interface SortState {
  field: SortField;
  direction: SortDirection;
  /** 目录是否始终排在文件之前（主流云盘默认开启） */
  foldersFirst: boolean;
}

export const DEFAULT_SORT_STATE: SortState = {
  field: "name",
  direction: "asc",
  foldersFirst: true,
};

export const SORT_FIELD_LABELS: Record<SortField, string> = {
  name: "名称",
  modified: "修改时间",
  size: "大小",
  type: "类型",
};

const collator = new Intl.Collator(["zh-Hans-CN", "en"], {
  numeric: true,
  sensitivity: "base",
});

function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  if (dot <= 0 || dot === name.length - 1) return "";
  return name.slice(dot + 1).toLowerCase();
}

function timestampOf(file: FileInfo): number {
  const raw = file.updated_at || file.created_at;
  if (!raw) return 0;
  const time = Date.parse(raw);
  return Number.isNaN(time) ? 0 : time;
}

function sizeOf(file: FileInfo): number {
  return typeof file.size_bytes === "number" && Number.isFinite(file.size_bytes)
    ? file.size_bytes
    : 0;
}

function compareByField(a: FileInfo, b: FileInfo, field: SortField): number {
  switch (field) {
    case "size":
      return sizeOf(a) - sizeOf(b);
    case "modified":
      return timestampOf(a) - timestampOf(b);
    case "type": {
      const extCompare = collator.compare(
        extensionOf(a.name),
        extensionOf(b.name),
      );
      if (extCompare !== 0) return extCompare;
      return collator.compare(a.name, b.name);
    }
    case "name":
    default:
      return collator.compare(a.name, b.name);
  }
}

/**
 * 返回排序后的新数组（不修改入参）。目录/文件分组的稳定性由
 * `foldersFirst` 与 `direction` 共同决定：方向只作用于组内排序，
 * 目录始终在上（当 `foldersFirst` 为真）。
 */
export function sortFiles<T extends FileInfo>(
  items: readonly T[],
  state: SortState,
): T[] {
  const direction = state.direction === "desc" ? -1 : 1;

  return [...items].sort((a, b) => {
    if (state.foldersFirst) {
      const aDir = a.kind === "directory";
      const bDir = b.kind === "directory";
      if (aDir !== bDir) return aDir ? -1 : 1;
    }

    const primary = compareByField(a, b, state.field) * direction;
    if (primary !== 0) return primary;

    // 稳定的次级排序：同值时按名称升序，避免顺序抖动
    return collator.compare(a.name, b.name);
  });
}

/**
 * 只对“真实条目”排序，保留当前目录/上级目录等快捷项在列表最前。
 */
export function sortBrowserItems<T extends FileInfo>(
  items: readonly T[],
  state: SortState,
  isShortcut: (item: T) => boolean = (item) =>
    Boolean((item as { uiRole?: string }).uiRole),
): T[] {
  const shortcuts: T[] = [];
  const rest: T[] = [];
  for (const item of items) {
    (isShortcut(item) ? shortcuts : rest).push(item);
  }
  return [...shortcuts, ...sortFiles(rest, state)];
}
