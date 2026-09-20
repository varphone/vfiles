import { computed, ref, type Ref } from "vue";
import { filesService } from "../services/files.service";
import { loadHighlight } from "../utils/highlight";

// 动态依赖只加载一次，后续预览复用
let cachedMarked: any | null = null;
let cachedHljs: any | null = null;

export type PreviewKind =
  | "text"
  | "image"
  | "markdown"
  | "code"
  | "pdf"
  | "video"
  | "audio"
  | "unsupported";

function getExtension(p: string): string {
  const name = p.split("/").pop() || "";
  const idx = name.lastIndexOf(".");
  if (idx <= 0 || idx === name.length - 1) return "";
  return name.slice(idx + 1).toLowerCase();
}

export function detectPreviewKind(filePath: string): PreviewKind {
  const ext = getExtension(filePath);
  const imageExts = new Set([
    "png",
    "jpg",
    "jpeg",
    "gif",
    "webp",
    "bmp",
    "svg",
  ]);
  if (imageExts.has(ext)) return "image";

  if (ext === "pdf") return "pdf";

  const videoExts = new Set(["mp4", "webm", "ogg", "mov", "m4v"]);
  if (videoExts.has(ext)) return "video";

  const audioExts = new Set(["mp3", "wav", "ogg", "m4a", "aac", "flac"]);
  if (audioExts.has(ext)) return "audio";

  const mdExts = new Set(["md", "markdown"]);
  if (mdExts.has(ext)) return "markdown";

  const codeExts = new Set([
    "js",
    "ts",
    "jsx",
    "tsx",
    "vue",
    "json",
    "css",
    "scss",
    "html",
    "xml",
    "yml",
    "yaml",
    "csv",
    "log",
    "sh",
    "py",
    "java",
    "c",
    "cpp",
    "go",
    "rs",
  ]);
  if (codeExts.has(ext)) return "code";

  const textExts = new Set(["txt", "log"]);
  if (textExts.has(ext) || ext === "") return "text";

  return "unsupported";
}

export function guessMimeByExt(filePath: string): string {
  const ext = getExtension(filePath);
  if (ext === "pdf") return "application/pdf";
  if (ext === "svg") return "image/svg+xml";
  if (ext === "png") return "image/png";
  if (ext === "jpg" || ext === "jpeg") return "image/jpeg";
  if (ext === "gif") return "image/gif";
  if (ext === "webp") return "image/webp";
  if (ext === "bmp") return "image/bmp";

  if (ext === "mp4" || ext === "m4v") return "video/mp4";
  if (ext === "webm") return "video/webm";
  if (ext === "mov") return "video/quicktime";
  if (ext === "ogg") return "application/ogg";

  if (ext === "mp3") return "audio/mpeg";
  if (ext === "wav") return "audio/wav";
  if (ext === "m4a") return "audio/mp4";
  if (ext === "aac") return "audio/aac";
  if (ext === "flac") return "audio/flac";

  return "application/octet-stream";
}

export function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export function safeLinkHref(href: string | null | undefined): string {
  const raw = (href || "").trim();
  if (!raw) return "#";
  if (raw.startsWith("#")) return raw;
  if (raw.startsWith("/")) return raw;
  if (/^https?:\/\//i.test(raw)) return raw;
  if (/^mailto:/i.test(raw)) return raw;
  return "#";
}

export function safeImageSrc(src: string | null | undefined): string {
  const raw = (src || "").trim();
  if (!raw) return "";
  if (/^https?:\/\//i.test(raw)) return raw;
  if (/^data:image\//i.test(raw)) return raw;
  if (raw.startsWith("/")) return raw;
  return "";
}

/**
 * 文件预览：根据扩展名选择文本/图片/PDF/音视频渲染方式，并按需加载
 * markdown 与代码高亮的重依赖（均为动态 import，不进入首屏包）。
 *
 * 从 FileBrowser 抽出，减小主组件体积并便于单测纯函数。
 */
export function useFilePreview(browseCommit: Ref<string | undefined>) {
  const preview = ref({
    open: false,
    loading: false,
    error: null as string | null,
    path: "",
    kind: "text" as PreviewKind,
    text: "",
    html: "",
    objectUrl: "",
  });

  const previewFilename = computed(
    () => preview.value.path.split("/").pop() || "file",
  );

  async function getMarked() {
    if (cachedMarked) return cachedMarked;
    const mod: any = await import("marked");
    const markedApi = mod?.marked ?? mod;

    const mdRenderer: any = {
      html(token: any) {
        const html =
          typeof token === "string" ? token : (token?.text ?? token?.raw ?? "");
        return escapeHtml(String(html));
      },
      link(tokenOrHref: any, title?: any, text?: any) {
        const href =
          tokenOrHref && typeof tokenOrHref === "object"
            ? tokenOrHref.href
            : tokenOrHref;
        const linkTitle =
          tokenOrHref && typeof tokenOrHref === "object"
            ? tokenOrHref.title
            : title;
        const linkText =
          tokenOrHref && typeof tokenOrHref === "object"
            ? tokenOrHref.text
            : text;

        const safeHref = safeLinkHref(href);
        const t = linkTitle ? ` title="${escapeHtml(String(linkTitle))}"` : "";
        const inner =
          typeof linkText === "string"
            ? (markedApi.parseInline(linkText) as string)
            : "";
        return `<a href="${escapeHtml(safeHref)}"${t} target="_blank" rel="noopener noreferrer">${inner}</a>`;
      },
      image(tokenOrHref: any, title?: any, text?: any) {
        const href =
          tokenOrHref && typeof tokenOrHref === "object"
            ? tokenOrHref.href
            : tokenOrHref;
        const imgTitle =
          tokenOrHref && typeof tokenOrHref === "object"
            ? tokenOrHref.title
            : title;
        const altText =
          tokenOrHref && typeof tokenOrHref === "object"
            ? tokenOrHref.text
            : text;

        const safeSrc = safeImageSrc(href);
        if (!safeSrc) return "";

        const t = imgTitle ? ` title="${escapeHtml(String(imgTitle))}"` : "";
        const alt = altText ? escapeHtml(String(altText)) : "";
        return `<img src="${escapeHtml(safeSrc)}" alt="${alt}" loading="lazy" decoding="async"${t} />`;
      },
    };

    markedApi.use({
      renderer: mdRenderer,
      gfm: true,
      breaks: true,
    });

    cachedMarked = markedApi;
    return markedApi;
  }

  async function getHljs() {
    if (cachedHljs) return cachedHljs;
    cachedHljs = await loadHighlight();
    return cachedHljs;
  }

  function closePreview() {
    if (preview.value.objectUrl) URL.revokeObjectURL(preview.value.objectUrl);
    preview.value = {
      open: false,
      loading: false,
      error: null,
      path: "",
      kind: "text",
      text: "",
      html: "",
      objectUrl: "",
    };
  }

  async function openPreview(filePath: string) {
    closePreview();
    preview.value.open = true;
    preview.value.loading = true;
    preview.value.path = filePath;
    preview.value.kind = detectPreviewKind(filePath);

    try {
      if (preview.value.kind === "unsupported") {
        preview.value.loading = false;
        return;
      }

      const blob = await filesService.getFileContent(
        filePath,
        browseCommit.value,
      );

      if (
        preview.value.kind === "image" ||
        preview.value.kind === "pdf" ||
        preview.value.kind === "video" ||
        preview.value.kind === "audio"
      ) {
        const typed = new Blob([await blob.arrayBuffer()], {
          type: guessMimeByExt(filePath),
        });
        preview.value.objectUrl = URL.createObjectURL(typed);
      } else {
        const text = await blob.text();
        if (preview.value.kind === "markdown") {
          const markedApi = await getMarked();
          preview.value.html = markedApi.parse(text) as string;
        } else if (preview.value.kind === "code") {
          const hljsApi = await getHljs();
          const highlighted = hljsApi.highlightAuto(text);
          preview.value.html = highlighted.value;
        } else {
          preview.value.text = text;
        }
      }
    } catch (err) {
      preview.value.error = err instanceof Error ? err.message : "预览失败";
    } finally {
      preview.value.loading = false;
    }
  }

  return {
    preview,
    previewFilename,
    closePreview,
    openPreview,
  };
}
