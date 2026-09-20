import { fireEvent, render, screen } from "@testing-library/vue";
import { beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import Notification from "../src/components/common/Notification.vue";
import { MAX_NOTIFICATIONS, useAppStore } from "../src/stores/app.store";

function renderToasts() {
  const pinia = createPinia();
  setActivePinia(pinia);
  render(Notification as any, { global: { plugins: [pinia] } });
  return useAppStore();
}

describe("Notification.vue", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("renders a toast per notification with an accessible live region", async () => {
    const store = renderToasts();

    store.success("上传完成");
    store.error("删除失败");

    expect(await screen.findByText("上传完成")).toBeInTheDocument();
    expect(screen.getByText("删除失败")).toBeInTheDocument();
    const region = screen.getByRole("status");
    expect(region).toHaveAttribute("aria-live", "polite");
  });

  it("closes a toast from its button", async () => {
    const store = renderToasts();
    store.warning("名称未变化");

    await screen.findByText("名称未变化");
    await fireEvent.click(screen.getByLabelText("关闭通知：名称未变化"));

    expect(screen.queryByText("名称未变化")).toBeNull();
  });

  it("keeps only the most recent toasts", async () => {
    const store = renderToasts();

    for (let index = 0; index < MAX_NOTIFICATIONS + 2; index += 1) {
      store.info(`通知 ${index}`);
    }

    expect(
      await screen.findByText(`通知 ${MAX_NOTIFICATIONS + 1}`),
    ).toBeInTheDocument();
    expect(screen.queryByText("通知 0")).toBeNull();
    expect(screen.queryByText("通知 1")).toBeNull();
  });
});
