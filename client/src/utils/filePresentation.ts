/**
 * 文件列表/网格共用的展示逻辑（大小、时间、类型、图标分类）。
 * 抽离出来避免网格视图与表格视图出现不一致的文案。
 */
import type { FileInfo } from "../types";

export type FileIconKind =
  | "parent"
  | "folder"
  | "image"
  | "video"
  | "audio"
  | "archive"
  | "code"
  | "text"
  | "pdf"
  | "file";

const IMAGE_EXTENSIONS = new Set([
  "jpg",
  "jpeg",
  "png",
  "gif",
  "svg",
  "webp",
  "bmp",
  "avif",
  "ico",
]);
const VIDEO_EXTENSIONS = new Set(["mp4", "webm", "mov", "mkv", "avi", "m4v"]);
const AUDIO_EXTENSIONS = new Set(["mp3", "wav", "flac", "aac", "ogg", "m4a"]);
const ARCHIVE_EXTENSIONS = new Set([
  "zip",
  "tar",
  "gz",
  "rar",
  "7z",
  "bz2",
  "xz",
]);
const CODE_EXTENSIONS = new Set([
  "js",
  "mjs",
  "cjs",
  "ts",
  "tsx",
  "jsx",
  "vue",
  "py",
  "java",
  "c",
  "h",
  "cpp",
  "hpp",
  "cs",
  "go",
  "rs",
  "rb",
  "php",
  "swift",
  "kt",
  "sql",
  "sh",
  "bash",
  "zsh",
  "json",
  "yaml",
  "yml",
  "toml",
  "xml",
  "html",
  "css",
  "scss",
  "less",
]);
const TEXT_EXTENSIONS = new Set(["txt", "md", "markdown", "log", "csv"]);

export function getExtension(name: string): string {
  const dot = name.lastIndexOf(".");
  if (dot <= 0 || dot === name.length - 1) return "";
  return name.slice(dot + 1).toLowerCase();
}

export function formatSize(bytes: number | undefined): string {
  const value = typeof bytes === "number" && Number.isFinite(bytes) ? bytes : 0;
  if (value <= 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(
    sizes.length - 1,
    Math.floor(Math.log(value) / Math.log(k)),
  );
  return `${(value / Math.pow(k, index)).toFixed(1)} ${sizes[index]}`;
}

/** 下载进度文案，如 " 42% (1.0 KB/2.4 KB)"。 */
export function formatDownloadProgress(loaded: number, total: number): string {
  if (total <= 0) return ` ${formatSize(loaded)}`;
  const percent = Math.floor((loaded / total) * 100);
  return ` ${percent}% (${formatSize(loaded)}/${formatSize(total)})`;
}

export function formatDate(date: string | undefined): string {
  if (!date) return "--";
  const parsed = new Date(date);
  if (Number.isNaN(parsed.getTime())) return date || "--";

  return parsed.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/**
 * 列表/网格用的相对时间：今天显示 "HH:mm"，昨天/本周显示 "昨天 HH:mm" / "N 天前"，
 * 更早显示日期。主流云盘都用这种方式降低信息密度，精确时间放在 title 里。
 */
export function formatRelativeDate(
  date: string | undefined,
  now: Date = new Date(),
): string {
  if (!date) return "--";
  const parsed = new Date(date);
  if (Number.isNaN(parsed.getTime())) return date || "--";

  const startOfDay = (value: Date) =>
    new Date(value.getFullYear(), value.getMonth(), value.getDate()).getTime();
  const dayDiff = Math.round(
    (startOfDay(now) - startOfDay(parsed)) / (24 * 60 * 60 * 1000),
  );
  const time = parsed.toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
  });

  if (dayDiff === 0) return `今天 ${time}`;
  if (dayDiff === 1) return `昨天 ${time}`;
  if (dayDiff > 1 && dayDiff < 7) return `${dayDiff} 天前`;

  return parsed.toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  });
}

export function fileKindLabel(file: FileInfo): string {
  if (file.kind === "directory") return "文件夹";

  const ext = getExtension(file.name);
  // 扩展名优先于 mime：浏览器/上传方常把 .ts 识别成 video/mp2t（TypeScript 源码），
  // 这类“视频”标签会严重误导用户。
  if (CODE_EXTENSIONS.has(ext)) return "代码文件";
  if (file.mime_type?.startsWith("image/")) return "图像文件";
  if (file.mime_type?.startsWith("video/")) return "视频文件";
  if (file.mime_type?.startsWith("audio/")) return "音频文件";
  if (ext === "pdf") return "PDF 文档";
  if (TEXT_EXTENSIONS.has(ext)) return "文本文档";
  if (ARCHIVE_EXTENSIONS.has(ext)) return "压缩文件";
  if (ext) return `${ext.toUpperCase()} 文件`;
  return "文件";
}

export function isImageFile(file: FileInfo): boolean {
  if (file.kind !== "file") return false;
  if (file.mime_type) return file.mime_type.startsWith("image/");
  return IMAGE_EXTENSIONS.has(getExtension(file.name));
}

export function fileIconKind(file: FileInfo): FileIconKind {
  if (file.kind === "directory") return "folder";

  // 同上：代码类扩展名优先，避免 .ts 被当作视频
  const extension = getExtension(file.name);
  if (CODE_EXTENSIONS.has(extension)) return "code";

  if (file.mime_type) {
    if (file.mime_type.startsWith("image/")) return "image";
    if (file.mime_type.startsWith("video/")) return "video";
    if (file.mime_type.startsWith("audio/")) return "audio";
    if (file.mime_type === "application/pdf") return "pdf";
  }

  const ext = getExtension(file.name);
  if (IMAGE_EXTENSIONS.has(ext)) return "image";
  if (VIDEO_EXTENSIONS.has(ext)) return "video";
  if (AUDIO_EXTENSIONS.has(ext)) return "audio";
  if (ARCHIVE_EXTENSIONS.has(ext)) return "archive";
  if (CODE_EXTENSIONS.has(ext)) return "code";
  if (TEXT_EXTENSIONS.has(ext)) return "text";
  if (ext === "pdf") return "pdf";
  return "file";
}

export interface NameSegment {
  text: string;
  match: boolean;
}

/** 将名称按搜索关键字切分，用于高亮命中的片段。 */
export function splitByNeedle(text: string, needleRaw: string): NameSegment[] {
  const hay = text ?? "";
  const needle = (needleRaw ?? "").trim().toLowerCase();
  if (!needle) return [{ text: hay, match: false }];

  const hayLower = hay.toLowerCase();
  const segments: NameSegment[] = [];
  let start = 0;

  while (start < hay.length) {
    const idx = hayLower.indexOf(needle, start);
    if (idx === -1) {
      segments.push({ text: hay.slice(start), match: false });
      break;
    }
    if (idx > start) {
      segments.push({ text: hay.slice(start, idx), match: false });
    }
    segments.push({ text: hay.slice(idx, idx + needle.length), match: true });
    start = idx + needle.length;
  }

  return segments.length ? segments : [{ text: hay, match: false }];
}
