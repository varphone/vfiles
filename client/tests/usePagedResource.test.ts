import { describe, expect, it } from "vitest";
import { usePagedResource } from "../src/composables/usePagedResource";

describe("usePagedResource", () => {
  it("页码越界时回退到最后一页", async () => {
    const calls: number[] = [];
    const resource = usePagedResource<number>({
      pageSize: 10,
      fetchPage: async ({ offset }) => {
        calls.push(offset);
        return { items: [offset], total: 25 };
      },
    });

    // 请求第 9 页（offset 90），服务端只有 3 页 → 回退到第 2 页（offset 20）。
    await resource.load(9);

    expect(resource.page.value).toBe(2);
    expect(calls).toEqual([90, 20]);
  });

  it("丢弃先发出、后返回的旧页结果", async () => {
    const pending: Array<(value: { items: number[]; total: number }) => void> = [];
    const resource = usePagedResource<number>({
      pageSize: 10,
      fetchPage: () =>
        new Promise((resolve) => {
          pending.push(resolve);
        }),
    });

    const first = resource.load(0);
    const second = resource.load(1);
    pending[1]({ items: [2], total: 20 });
    await second;
    pending[0]({ items: [1], total: 20 });
    await first;

    expect(resource.items.value).toEqual([2]);
    expect(resource.page.value).toBe(1);
  });
});
