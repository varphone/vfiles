import { describe, expect, it } from "vitest";
import { useListResource } from "../src/composables/useListResource";

describe("useListResource", () => {
  it("丢弃先发出、后返回的旧请求结果", async () => {
    let call = 0;
    const pending: Array<(value: { items: number[]; total: number }) => void> = [];
    const resource = useListResource<number>({
      pageSize: 2,
      fetchPage: () =>
        new Promise((resolve) => {
          call += 1;
          pending.push(resolve);
        }),
    });

    const first = resource.reload();
    const second = resource.reload();
    // 后发出的请求先返回，先发出的请求后返回（模拟乱序）。
    pending[1]({ items: [2], total: 1 });
    await second;
    pending[0]({ items: [1], total: 1 });
    await first;

    expect(resource.items.value).toEqual([2]);
  });

  it("追加时按 key 去重并更新 total", async () => {
    const pages = [
      { items: [{ id: "a" }, { id: "b" }], total: 3 },
      { items: [{ id: "b" }, { id: "c" }], total: 3 },
    ];
    const resource = useListResource<{ id: string }>({
      pageSize: 2,
      key: (item) => item.id,
      fetchPage: async () => pages.shift() ?? { items: [], total: 3 },
    });

    await resource.reload();
    await resource.loadMore();

    expect(resource.items.value.map((item) => item.id)).toEqual(["a", "b", "c"]);
    expect(resource.total.value).toBe(3);
  });

  it("失败时记录错误并复位加载状态", async () => {
    const resource = useListResource<number>({
      fetchPage: async () => {
        throw new Error("网络错误");
      },
    });

    await resource.reload();

    expect(resource.error.value).toBe("网络错误");
    expect(resource.loading.value).toBe(false);
  });
});
