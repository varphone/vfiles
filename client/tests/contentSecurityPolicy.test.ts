import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const indexHtml = readFileSync(resolve(process.cwd(), "index.html"), "utf8");
const themeInitializer = readFileSync(
  resolve(process.cwd(), "public/theme-init.js"),
  "utf8",
);

describe("frontend Content Security Policy compatibility", () => {
  it("loads the first-paint theme initializer without inline scripts", () => {
    expect(indexHtml).toContain('<script src="/theme-init.js"></script>');
    expect(indexHtml).not.toMatch(
      /<script\b(?![^>]*\bsrc=)[^>]*>[\s\S]*?<\/script>/i,
    );
    expect(themeInitializer).toContain('localStorage.getItem("vfiles:theme")');
  });
});
