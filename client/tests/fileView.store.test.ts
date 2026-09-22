import { beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { nextTick } from "vue";
import {
  DEFAULT_COLUMN_WIDTHS,
  useFileViewStore,
  cardMinWidth,
} from "../src/stores/fileView.store";

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

  it("defaults the details panel to visible and persists changes", async () => {
    const store = useFileViewStore();
    expect(store.detailsVisible).toBe(true);

    store.toggleDetails();
    expect(store.detailsVisible).toBe(false);

    // persist() 走 post-flush watcher，等一个 tick 再断言落盘结果
    await nextTick();
    const persisted = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    expect(persisted.detailsVisible).toBe(false);

    store.setDetailsVisible(true);
    expect(store.detailsVisible).toBe(true);
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

  it("stores column widths with clamping and resets to defaults", () => {
    const store = useFileViewStore();
    // 初始不存宽度：名称列走"自适应吸收余量"，其余列回落默认值渲染
    expect(store.columnWidths.name).toBeUndefined();

    store.setColumnWidth("name", 400);
    expect(store.columnWidths.name).toBe(400);

    // 过窄/过宽都被夹到允许范围
    store.setColumnWidth("name", 10);
    expect(store.columnWidths.name).toBe(88);
    store.setColumnWidth("name", 5000);
    expect(store.columnWidths.name).toBe(640);

    // 复位 = 清除存储（名称列回到自适应；其余列回落默认渲染）
    store.resetColumnWidth("name");
    expect(store.columnWidths.name).toBeUndefined();
  });

  it("persists column widths and restores them", async () => {
    const store = useFileViewStore();
    store.setColumnWidth("modified", 260);
    await nextTick();

    const persisted = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    expect(persisted.columnWidths.modified).toBe(260);

    setActivePinia(createPinia());
    const restored = useFileViewStore();
    expect(restored.columnWidths.modified).toBe(260);
    // 其它列回到默认值
    expect(restored.columnWidths.name).toBe(DEFAULT_COLUMN_WIDTHS.name);
  });

  it("falls back to defaults on corrupt storage", () => {
    localStorage.setItem(STORAGE_KEY, "{not-json");
    setActivePinia(createPinia());
    const store = useFileViewStore();
    expect(store.mode).toBe("list");
    expect(store.sortField).toBe("name");
  });
});

describe("cardMinWidth", () => {
  it("derives the grid min column width from the thumbnail size", () => {
    expect(cardMinWidth(144)).toBe(194);
    expect(cardMinWidth(200)).toBe(270);
    expect(cardMinWidth(100)).toBe(135);
  });
});
