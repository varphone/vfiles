import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import NotificationCenter from "../src/components/common/NotificationCenter.vue";
import { useAppStore } from "../src/stores/app.store";

function renderCenter() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const utils = render(NotificationCenter as any, {
    global: { plugins: [pinia] },
  });
  return { ...utils, store: useAppStore() };
}

describe("NotificationCenter.vue", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("shows an unread badge that clears when opened", async () => {
    const { container, store } = renderCenter();

    store.success("上传完成");
    store.error("上传失败：网络错误");
    await waitFor(() =>
      expect(
        container.querySelector(".app-bar-bell-badge")?.textContent?.trim(),
      ).toBe("2"),
    );

    await fireEvent.click(screen.getByLabelText(/通知/));
    await waitFor(() =>
      expect(container.querySelector(".app-bar-bell-badge")).toBeNull(),
    );
    expect(store.unreadNotifications).toBe(0);
  });

  it("lists notification history newest first with relative time", async () => {
    const { container, store } = renderCenter();

    store.info("先发生的");
    store.warning("后发生的");
    await fireEvent.click(screen.getByLabelText(/通知/));

    await waitFor(() =>
      expect(
        container.querySelectorAll(".notification-center-item"),
      ).toHaveLength(2),
    );
    const messages = Array.from(
      container.querySelectorAll(".notification-center-message"),
    ).map((el) => el.textContent?.trim());
    // 最新的排在最前
    expect(messages).toEqual(["后发生的", "先发生的"]);
    expect(
      container.querySelector(".notification-center-time")?.textContent,
    ).toContain("今天");
  });

  it("keeps dismissed toasts in the history", async () => {
    const { store } = renderCenter();

    store.error("失败了");
    // toast 自动消失（模拟定时清理），历史仍应保留
    const [entry] = store.notifications;
    store.removeNotification(entry.id);

    expect(store.notifications).toHaveLength(0);
    expect(store.notificationHistory).toHaveLength(1);
    expect(store.notificationHistory[0].message).toBe("失败了");
  });

  it("clears the history and shows the empty state", async () => {
    const { container, store } = renderCenter();

    store.success("清理目标");
    await fireEvent.click(screen.getByLabelText(/通知/));
    await screen.findByText("清理目标");

    await fireEvent.click(screen.getByText("清空"));
    await waitFor(() =>
      expect(screen.getByText("暂无通知")).toBeInTheDocument(),
    );
    expect(store.notificationHistory).toHaveLength(0);
    expect(container.querySelector(".notification-center-list")).toBeNull();
  });

  it("closes on Escape and outside click", async () => {
    const { container } = renderCenter();

    const open = async () => {
      await fireEvent.click(screen.getByLabelText(/通知/));
      await waitFor(() =>
        expect(
          container.querySelector(".notification-center.is-active"),
        ).not.toBeNull(),
      );
    };

    await open();
    await fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() =>
      expect(
        container.querySelector(".notification-center.is-active"),
      ).toBeNull(),
    );

    await open();
    await fireEvent.click(document.body);
    await waitFor(() =>
      expect(
        container.querySelector(".notification-center.is-active"),
      ).toBeNull(),
    );
  });
});
