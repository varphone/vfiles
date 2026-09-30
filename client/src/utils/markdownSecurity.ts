/** HTML escaping and URL policies shared by both Markdown preview surfaces. */
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

const INLINE_RASTER_IMAGE =
  /^data:image\/(?:avif|bmp|gif|jpe?g|png|webp);base64,[a-z0-9+/]*={0,2}$/i;

/**
 * Markdown images load without a user gesture, so only allow same-origin paths
 * and inline raster data. Remote images can track preview viewers or probe
 * private network hosts; SVG data URLs can contain active content.
 */
export function safeImageSrc(src: string | null | undefined): string {
  const raw = (src || "").trim();
  const hasUnsafeCharacter = [...raw].some((character) => {
    const code = character.charCodeAt(0);
    return character === "\\" || code <= 0x1f || code === 0x7f;
  });
  if (!raw || hasUnsafeCharacter) return "";
  if (INLINE_RASTER_IMAGE.test(raw)) return raw;
  if (raw.startsWith("/") && !raw.startsWith("//")) return raw;
  return "";
}
