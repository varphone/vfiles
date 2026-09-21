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

    await fireEvent.click(screen.getByRole("button", { name: /^通知/ }));
    await waitFor(() =>
      expect(container.querySelector(".app-bar-bell-badge")).toBeNull(),
    );
    expect(store.unreadNotifications).toBe(0);
  });

  it("lists notification history newest first with relative time", async () => {
    const { container, store } = renderCenter();

    store.info("先发生的");
    store.warning("后发生的");
    await fireEvent.click(screen.getByRole("button", { name: /^通知/ }));

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
    await fireEvent.click(screen.getByRole("button", { name: /^通知/ }));
    await screen.findByText("清理目标");

    await fireEvent.click(screen.getByText("清空"));
    await waitFor(() =>
      expect(screen.getByText("暂无通知")).toBeInTheDocument(),
    );
    expect(store.notificationHistory).toHaveLength(0);
    expect(container.querySelector(".notification-center-list")).toBeNull();
  });

  it("filters the history by type", async () => {
    const { container, store } = renderCenter();

    store.success("成功了");
    store.error("失败了");
    store.warning("注意点");
    await fireEvent.click(screen.getByRole("button", { name: /^通知/ }));
    await waitFor(() =>
      expect(
        container.querySelectorAll(".notification-center-item"),
      ).toHaveLength(3),
    );

    // 默认「全部」激活
    expect(
      container
        .querySelector(".notification-center-filter.is-active")
        ?.textContent?.trim(),
    ).toBe("全部");

    await fireEvent.click(screen.getByRole("tab", { name: "失败" }));
    await waitFor(() =>
      expect(
        container.querySelectorAll(".notification-center-item"),
      ).toHaveLength(1),
    );
    expect(
      container.querySelector(".notification-center-message")?.textContent,
    ).toBe("失败了");

    // 没有该类型时给出对应空态
    store.clearNotificationHistory();
    store.info("只有信息");
    await fireEvent.click(screen.getByRole("tab", { name: "成功" }));
    await waitFor(() =>
      expect(screen.getByText("没有成功通知")).toBeInTheDocument(),
    );
  });

  it("removes a single notification from the history", async () => {
    const { container, store } = renderCenter();

    store.success("保留这条");
    store.error("删掉这条");
    await fireEvent.click(screen.getByRole("button", { name: /^通知/ }));
    await waitFor(() =>
      expect(
        container.querySelectorAll(".notification-center-item"),
      ).toHaveLength(2),
    );

    const removeButton = container.querySelector<HTMLButtonElement>(
      '.notification-center-remove[aria-label="移除通知：删掉这条"]',
    )!;
    await fireEvent.click(removeButton);

    await waitFor(() =>
      expect(
        container.querySelectorAll(".notification-center-item"),
      ).toHaveLength(1),
    );
    expect(
      container.querySelector(".notification-center-message")?.textContent,
    ).toBe("保留这条");
    expect(store.notificationHistory).toHaveLength(1);
  });

  it("closes on Escape and outside click", async () => {
    const { container } = renderCenter();

    const open = async () => {
      await fireEvent.click(screen.getByRole("button", { name: /^通知/ }));
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
