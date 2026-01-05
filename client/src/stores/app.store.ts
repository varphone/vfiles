import { defineStore } from "pinia";
import { ref } from "vue";

interface Notification {
  id: number;
  type: "success" | "error" | "warning" | "info";
  message: string;
}

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

  return {
    notifications,
    showNotification,
    removeNotification,
    success,
    error,
    warning,
    info,
  };
});
