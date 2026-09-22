import { defineStore } from "pinia";
import { ref } from "vue";

interface Notification {
  id: number;
  type: "success" | "error" | "warning" | "info";
  message: string;
  /** 产生时间（通知中心显示「x 分钟前」）。 */
  createdAt: number;
}

/** 同时保留的通知条数上限：超出时丢弃最旧的一条，避免批量操作刷屏。 */
export const MAX_NOTIFICATIONS = 4;

/** 通知中心保留的历史条数（已消失的提示也能回看）。 */
export const MAX_NOTIFICATION_HISTORY = 50;

export const useAppStore = defineStore("app", () => {
  const notifications = ref<Notification[]>([]);
  /** 通知历史：即使 toast 已自动消失也保留，供「通知中心」回看。 */
  const notificationHistory = ref<Notification[]>([]);
  /** 通知中心里未读的条数（打开面板后清零）。 */
  const unreadNotifications = ref(0);
  let notificationId = 0;

  function showNotification(
    type: Notification["type"],
    message: string,
    duration = 3000,
  ) {
    const id = notificationId++;
    const entry: Notification = { id, type, message, createdAt: Date.now() };
    notifications.value.push(entry);

    // 历史按时间倒序（最新在前）保留最近 N 条
    notificationHistory.value = [entry, ...notificationHistory.value].slice(
      0,
      MAX_NOTIFICATION_HISTORY,
    );
    unreadNotifications.value += 1;

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

  /** 打开通知中心：清空未读计数。 */
  function markNotificationsRead() {
    unreadNotifications.value = 0;
  }

  /** 从历史里移除单条（toast 若还在也一并移除）。 */
  function removeNotificationFromHistory(id: number) {
    notificationHistory.value = notificationHistory.value.filter(
      (item) => item.id !== id,
    );
    removeNotification(id);
  }

  /** 清空通知历史。 */
  function clearNotificationHistory() {
    notificationHistory.value = [];
    unreadNotifications.value = 0;
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

  // 时长分级（r123 ✓ 主流惯例：成功/信息 3s、警告 6s、错误 8s（需读完））
  function error(message: string) {
    showNotification("error", message, 8000);
  }

  function warning(message: string) {
    showNotification("warning", message, 6000);
  }

  function info(message: string) {
    showNotification("info", message);
  }

  /**
   * 「新建文件夹」请求计数。
   *
   * 移动端底栏位于 Home，而创建目录的对话框在 FileBrowser 内，
   * 这里用一个自增计数做单向投递，避免把目录状态提升到页面级。
   */
  const createDirectoryRequests = ref(0);

  function requestCreateDirectory() {
    createDirectoryRequests.value += 1;
  }

  return {
    notifications,
    notificationHistory,
    unreadNotifications,
    markNotificationsRead,
    removeNotificationFromHistory,
    clearNotificationHistory,
    createDirectoryRequests,
    requestCreateDirectory,
    showNotification,
    removeNotification,
    success,
    error,
    warning,
    info,
  };
});
