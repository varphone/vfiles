import { describe, expect, it } from "vitest";
import {
  escapeHtml,
  safeImageSrc,
  safeLinkHref,
} from "../src/utils/markdownSecurity";

describe("markdownSecurity", () => {
  it("escapes markup characters before rendering Markdown HTML", () => {
    expect(escapeHtml(`<tag title="x">'&`)).toBe(
      "&lt;tag title=&quot;x&quot;&gt;&#39;&amp;",
    );
  });

  it("allows fragments, same-origin paths, and explicit safe link schemes", () => {
    expect(safeLinkHref("#section")).toBe("#section");
    expect(safeLinkHref("/files/a%20b.md")).toBe("/files/a%20b.md");
    expect(safeLinkHref("https://docs.example.test/guide")).toBe(
      "https://docs.example.test/guide",
    );
    expect(safeLinkHref("mailto:help@example.test")).toBe(
      "mailto:help@example.test",
    );
  });

  it("rejects protocol-relative, backslash, control-character, and deceptive URLs", () => {
    for (const href of [
      "//evil.example/track",
      "///evil.example/track",
      "/\\\\evil.example/path",
      "/files/\nnext",
      "javascript:alert(1)",
      "data:text/html,<script>alert(1)</script>",
      "https://user:password@docs.example.test/",
      "https://",
    ]) {
      expect(safeLinkHref(href), href).toBe("#");
    }
  });

  it("allows only local or inline raster images", () => {
    expect(safeImageSrc("/files/image.png")).toBe("/files/image.png");
    expect(safeImageSrc("//tracker.example/pixel.png")).toBe("");
    expect(safeImageSrc("data:image/svg+xml;base64,PHN2Zz4=")).toBe("");
  });
});
