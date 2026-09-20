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

export function fileKindLabel(file: FileInfo): string {
  if (file.kind === "directory") return "文件夹";

  const ext = getExtension(file.name);
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
