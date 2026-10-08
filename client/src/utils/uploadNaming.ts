/**
 * 上传命名工具（A 决策「保留两个」= Dropbox 式自动改名 ✗ 纯函数单测）。
 */

/** 拆文件名为主体与扩展名（点文件如 `.env` 整体为主体 ✓ 无扩展）。 */
export function splitName(name: string): { base: string; ext: string } {
  const idx = name.lastIndexOf(".");
  if (idx <= 0) return { base: name, ext: "" };
  return { base: name.slice(0, idx), ext: name.slice(idx) };
}

/**
 * 从浏览器的 `webkitRelativePath` 提取上传目标的相对目录。
 * 普通文件没有相对路径，返回空字符串。
 */
export function uploadRelativeDirectory(relativePath: string): string {
  const normalized = relativePath.replace(/\\/g, "/").replace(/^\/+|\/+$/g, "");
  const separator = normalized.lastIndexOf("/");
  return separator < 0 ? "" : normalized.slice(0, separator);
}

/**
 * 生成本次目录上传中的目标相对路径，用于按完整路径判断重名。
 */
export function uploadTargetKey(
  relativePath: string,
  filename: string,
): string {
  const directory = uploadRelativeDirectory(relativePath);
  return directory ? `${directory}/${filename}` : filename;
}

/**
 * 「保留两个」改名：从 `名 (1).ext` 起找首个未占用名（含已 taken ∪ 目录既存 ✗）。
 */
export function keepBothName(name: string, taken: ReadonlySet<string>): string {
  const { base, ext } = splitName(name);
  if (!taken.has(name)) return name;
  for (let i = 1; i < 10000; i += 1) {
    const candidate = `${base} (${i})${ext}`;
    if (!taken.has(candidate)) return candidate;
  }
  // 理论不可达：退回带时间戳的唯一后缀（防御）
  return `${base} (${Date.now()})${ext}`;
}
