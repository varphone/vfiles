import type { FileInfo } from "../types";

/**
 * 文件浏览器使用的纯路径工具：不依赖 Vue 或服务，便于单测。
 */

/** 目录/条目名是否安全（非空、非 `.`/`..`、不含路径分隔符）。 */
export function isSafeDirName(name: string): boolean {
  const trimmed = name.trim();
  if (!trimmed) return false;
  if (trimmed === "." || trimmed === "..") return false;
  if (trimmed.includes("/") || trimmed.includes("\\")) return false;
  return true;
}

export function buildChildPath(parentPath: string, name: string): string {
  return parentPath ? `${parentPath}/${name}` : name;
}

export function parentDirectoryPath(path: string): string {
  const parts = path.split("/").filter(Boolean);
  parts.pop();
  return parts.join("/");
}

export function buildSiblingPath(path: string, name: string): string {
  const parent = parentDirectoryPath(path);
  return parent ? `${parent}/${name}` : name;
}

/** 规整用户输入的目录：去空白、统一分隔符、去首尾斜杠。 */
export function normalizeTargetDirectory(rawPath: string): string {
  return rawPath
    .trim()
    .replace(/\\/g, "/")
    .replace(/^\/+/, "")
    .replace(/\/+$/, "");
}

/** 条目在目标目录下的新路径；非法或未变化时抛出可展示的错误。 */
export function resolveMoveTargetPath(
  file: FileInfo,
  targetDir: string,
): string {
  if (
    file.kind === "directory" &&
    (targetDir === file.path || targetDir.startsWith(`${file.path}/`))
  ) {
    throw new Error("不能将目录移动到自身或其子目录");
  }

  const to = buildChildPath(targetDir, file.name);
  if (to === file.path) {
    throw new Error("目标目录未变化");
  }

  return to;
}

/**
 * 规划一次（批量）移动：校验目标重名（批内互撞或与目标目录已有条目重名）。
 */
export function planMoveOperations(
  items: FileInfo[],
  targetDir: string,
  targetEntries: Pick<FileInfo, "path">[] = [],
): Array<{ file: FileInfo; to: string }> {
  const usedTargets = new Set<string>();
  const existingPaths = new Set(targetEntries.map((entry) => entry.path));

  return items.map((file) => {
    const to = resolveMoveTargetPath(file, targetDir);
    if (usedTargets.has(to)) {
      throw new Error(`目标目录中会产生重名项：${file.name}`);
    }
    if (existingPaths.has(to)) {
      throw new Error(`目标目录已存在同名项目：${file.name}`);
    }
    usedTargets.add(to);
    return { file, to };
  });
}
