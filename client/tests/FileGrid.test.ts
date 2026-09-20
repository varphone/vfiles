import { describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen } from "@testing-library/vue";
import FileGrid from "../src/components/file-browser/FileGrid.vue";
import type { FileInfo } from "../src/types";

function buildFile(overrides: Partial<FileInfo> = {}): FileInfo {
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

function renderGrid(files: FileInfo[]) {
  return render(FileGrid as any, {
    global: { plugins: [createPinia()] },
    props: {
      files,
      selectMode: false,
      selectedPaths: new Set<string>(),
    },
  });
}

describe("FileGrid.vue", () => {
  it("renders a card for every entry with name and size metadata", () => {
    setActivePinia(createPinia());
    renderGrid([
      buildFile({ id: "a", name: "photo.png", size_bytes: 2048 }),
      buildFile({
        id: "b",
        name: "docs",
        path: "docs",
        kind: "directory",
      }),
    ]);

    expect(screen.getByText("photo.png")).toBeInTheDocument();
    expect(screen.getByText("docs")).toBeInTheDocument();
    expect(screen.getByText("2.0 KB")).toBeInTheDocument();
  });

  it("shows a checkbox per card in selection mode", () => {
    setActivePinia(createPinia());
    render(FileGrid as any, {
      global: { plugins: [createPinia()] },
      props: {
        files: [buildFile({ name: "a.txt" })],
        selectMode: true,
        selectedPaths: new Set<string>(),
      },
    });

    expect(screen.getByLabelText("选择 a.txt")).toBeInTheDocument();
  });
});
