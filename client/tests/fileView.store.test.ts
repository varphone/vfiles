import { beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useFileViewStore } from "../src/stores/fileView.store";

const STORAGE_KEY = "vfiles:file-browser:view";

/** 测试环境（bun + jsdom）的 localStorage 不完整，这里提供确定性实现。 */
function installLocalStorageStub() {
  const backing = new Map<string, string>();
  const stub = {
    getItem: (key: string) => (backing.has(key) ? backing.get(key)! : null),
    setItem: (key: string, value: string) => {
      backing.set(key, String(value));
    },
    removeItem: (key: string) => {
      backing.delete(key);
    },
    clear: () => {
      backing.clear();
    },
    key: (index: number) => Array.from(backing.keys())[index] ?? null,
    get length() {
      return backing.size;
    },
  };
  Object.defineProperty(globalThis, "localStorage", {
    value: stub,
    configurable: true,
    writable: true,
  });
}

describe("fileView store", () => {
  beforeEach(() => {
    installLocalStorageStub();
    setActivePinia(createPinia());
  });

  it("defaults to list view with name ascending sorting", () => {
    const store = useFileViewStore();
    expect(store.mode).toBe("list");
    expect(store.sortField).toBe("name");
    expect(store.sortDirection).toBe("asc");
    expect(store.foldersFirst).toBe(true);
  });

  it("toggles view mode and sort direction", () => {
    const store = useFileViewStore();
    store.toggleMode();
    expect(store.mode).toBe("grid");
    store.toggleSortDirection();
    expect(store.sortDirection).toBe("desc");
    store.toggleFoldersFirst();
    expect(store.foldersFirst).toBe(false);
  });

  it("ignores invalid values", () => {
    const store = useFileViewStore();
    store.setMode("invalid" as never);
    store.setSortField("invalid" as never);
    expect(store.mode).toBe("list");
    expect(store.sortField).toBe("name");
  });

  it("clamps the thumbnail size to the supported range", () => {
    const store = useFileViewStore();
    store.setThumbnailSize(5);
    expect(store.thumbnailSize).toBe(96);
    store.setThumbnailSize(9999);
    expect(store.thumbnailSize).toBe(240);
  });

  it("restores persisted preferences", () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        mode: "grid",
        sortField: "size",
        sortDirection: "desc",
        foldersFirst: false,
        thumbnailSize: 200,
      }),
    );

    setActivePinia(createPinia());
    const store = useFileViewStore();
    expect(store.mode).toBe("grid");
    expect(store.sortField).toBe("size");
    expect(store.sortDirection).toBe("desc");
    expect(store.foldersFirst).toBe(false);
    expect(store.thumbnailSize).toBe(200);
  });

  it("falls back to defaults on corrupt storage", () => {
    localStorage.setItem(STORAGE_KEY, "{not-json");
    setActivePinia(createPinia());
    const store = useFileViewStore();
    expect(store.mode).toBe("list");
    expect(store.sortField).toBe("name");
  });
});
