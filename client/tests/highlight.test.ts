import { describe, expect, it, vi } from "vitest";
import {
  hasFencedCodeBlock,
  highlightCode,
  languageForPath,
  renderHighlightedCode,
  type HighlightApi,
} from "../src/utils/highlight";

function fakeApi(overrides: Partial<HighlightApi> = {}): HighlightApi {
  return {
    highlightAuto: vi.fn(() => ({ value: "AUTO" })),
    highlight: vi.fn(() => ({ value: "MANUAL" })),
    getLanguage: vi.fn(() => true),
    ...overrides,
  };
}

describe("languageForPath", () => {
  it("maps known extensions to highlight.js languages", () => {
    expect(languageForPath("src/main.ts")).toBe("typescript");
    expect(languageForPath("src/App.vue")).toBe("xml");
    expect(languageForPath("docs/readme.MD")).toBe("markdown");
    expect(languageForPath("crates/domain/tests/a.rs")).toBe("rust");
    expect(languageForPath("Dockerfile")).toBe("dockerfile");
    expect(languageForPath("Makefile")).toBe("makefile");
  });

  it("returns an empty language for unknown or extension-less files", () => {
    expect(languageForPath("LICENSE")).toBe("");
    expect(languageForPath("archive.unknown")).toBe("");
    expect(languageForPath("trailing.")).toBe("");
  });
});

describe("hasFencedCodeBlock", () => {
  it("detects backtick and tilde fences (including indented ones)", () => {
    expect(hasFencedCodeBlock("text\n```js\nlet a = 1;\n```")).toBe(true);
    expect(hasFencedCodeBlock("~~~\ncode\n~~~")).toBe(true);
    expect(hasFencedCodeBlock("  ```\ncode\n  ```")).toBe(true);
  });

  it("ignores inline code and plain paragraphs", () => {
    expect(hasFencedCodeBlock("use `npm run build` here")).toBe(false);
    expect(hasFencedCodeBlock("# Title\n\nplain text")).toBe(false);
  });
});

describe("highlightCode", () => {
  it("uses the requested language when it is available", () => {
    const api = fakeApi();
    expect(highlightCode(api, "const a = 1;", "typescript").value).toBe(
      "MANUAL",
    );
    expect(api.highlight).toHaveBeenCalledWith("const a = 1;", {
      language: "typescript",
      ignoreIllegals: true,
    });
    expect(api.highlightAuto).not.toHaveBeenCalled();
  });

  it("falls back to auto detection for unknown languages", () => {
    const api = fakeApi({ getLanguage: vi.fn(() => false) });
    expect(highlightCode(api, "nope", "brainfuck").value).toBe("AUTO");
    expect(api.highlight).not.toHaveBeenCalled();
  });

  it("ignores a missing getLanguage implementation", () => {
    const api = fakeApi({ getLanguage: undefined });
    expect(highlightCode(api, "code", "python").value).toBe("AUTO");
  });
});

describe("renderHighlightedCode", () => {
  it("wraps the highlighted output with the language class", () => {
    const html = renderHighlightedCode(fakeApi(), "let a = 1;", "javascript");
    expect(html).toBe(
      '<pre><code class="hljs language-javascript">MANUAL</code></pre>',
    );
  });

  it("omits the language class when nothing was detected", () => {
    const html = renderHighlightedCode(fakeApi(), "plain", "");
    expect(html).toBe('<pre><code class="hljs">AUTO</code></pre>');
  });

  it("sanitizes the language token from the markdown fence", () => {
    const api = fakeApi({ getLanguage: vi.fn(() => false) });
    const html = renderHighlightedCode(
      api,
      "code",
      '"><img src=x onerror=alert(1)>',
    );
    expect(html).not.toContain("<img");
    expect(html).toContain('class="hljs language-img"');
  });
});
