import { render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import FileList from "../src/components/file-browser/FileList.vue";

function buildFile(overrides: Record<string, unknown> = {}) {
  return {
    id: "file-id",
    name: "entry",
    path: "entry",
    kind: "file",
    created_at: "2026-05-29T00:00:00.000Z",
    updated_at: "2026-05-29T00:00:00.000Z",
    ...overrides,
  };
}

describe("FileList.vue", () => {
  it("renders directory and shortcut names as links in desktop mode", () => {
    render(FileList as any, {
      props: {
        desktop: true,
        selectMode: false,
        selectedPaths: new Set<string>(),
        files: [
          buildFile({
            id: "self-id",
            name: ".",
            path: "__vfiles_shortcut_self__:root",
            kind: "directory",
            uiRole: "self",
          }),
          buildFile({
            id: "parent-id",
            name: "..",
            path: "__vfiles_shortcut_parent__:root",
            kind: "directory",
            uiRole: "parent",
          }),
          buildFile({
            id: "dir-id",
            name: "docs",
            path: "docs",
            kind: "directory",
          }),
          buildFile({
            id: "file-id",
            name: "readme.txt",
            path: "readme.txt",
            kind: "file",
          }),
        ],
      },
    });

    expect(screen.getByRole("link", { name: "." })).toHaveClass(
      "has-text-link",
    );
    expect(screen.getByRole("link", { name: ".." })).toHaveClass(
      "has-text-link",
    );
    expect(screen.getByRole("link", { name: "docs" })).toHaveClass(
      "has-text-link",
    );
    expect(
      screen.queryByRole("link", { name: "readme.txt" }),
    ).not.toBeInTheDocument();
  });
});