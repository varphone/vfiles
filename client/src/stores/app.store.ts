import { defineStore } from "pinia";
import { ref } from "vue";

interface Notification {
  id: number;
  type: "success" | "error" | "warning" | "info";
  message: string;
}

/** 同时保留的通知条数上限：超出时丢弃最旧的一条，避免批量操作刷屏。 */
export const MAX_NOTIFICATIONS = 4;

export const useAppStore = defineStore("app", () => {
  const notifications = ref<Notification[]>([]);
  let notificationId = 0;

  function showNotification(
    type: Notification["type"],
    message: string,
    duration = 3000,
  ) {
    const id = notificationId++;
    notifications.value.push({ id, type, message });

    // 批量操作可能一次产生多条通知，这里只保留最近的几条
    if (notifications.value.length > MAX_NOTIFICATIONS) {
      notifications.value = notifications.value.slice(-MAX_NOTIFICATIONS);
    }

    if (duration > 0) {
      setTimeout(() => {
        removeNotification(id);
      }, duration);
    }
  }

  function removeNotification(id: number) {
    const index = notifications.value.findIndex((n) => n.id === id);
    if (index !== -1) {
      notifications.value.splice(index, 1);
    }
  }

  function success(message: string) {
    showNotification("success", message);
  }

  function error(message: string) {
    showNotification("error", message, 5000);
  }

  function warning(message: string) {
    showNotification("warning", message);
  }

  function info(message: string) {
    showNotification("info", message);
  }

  /**
   * 应用栏「全局搜索」→ 文件浏览器的单向通道。
   *
   * 搜索状态仍由 FileBrowser 持有（结果渲染、分页、过期响应处理都在那里），
   * 应用栏只负责把一个查询请求投递过去，避免把整套搜索状态提升到页面级。
   */
  const pendingSearch = ref<string | null>(null);

  function requestSearch(query: string) {
    const trimmed = query.trim();
    pendingSearch.value = trimmed.length > 0 ? trimmed : null;
  }

  function clearPendingSearch() {
    pendingSearch.value = null;
  }

  return {
    notifications,
    pendingSearch,
    requestSearch,
    clearPendingSearch,
    showNotification,
    removeNotification,
    success,
    error,
    warning,
    info,
  };
});
