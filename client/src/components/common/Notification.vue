<template>
  <div class="notifications">
    <transition-group name="notification">
      <div
        v-for="notification in notifications"
        :key="notification.id"
        :class="['notification', `is-${notification.type}`]"
      >
        <button class="delete" @click="remove(notification.id)"></button>
        {{ notification.message }}
      </div>
    </transition-group>
  </div>
</template>

<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAppStore } from "../../stores/app.store";

const appStore = useAppStore();
const { notifications } = storeToRefs(appStore);

function remove(id: number) {
  appStore.removeNotification(id);
}
</script>

<style scoped>
.notifications {
  position: fixed;
  top: 1rem;
  right: 1rem;
  z-index: 10000;
  max-width: 400px;
  width: 100%;
}

.notification {
  margin-bottom: 0.5rem;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
}

.notification-enter-active,
.notification-leave-active {
  transition: all 0.3s ease;
}

.notification-enter-from {
  opacity: 0;
  transform: translateX(100%);
}

.notification-leave-to {
  opacity: 0;
  transform: translateX(100%);
}

@media screen and (max-width: 768px) {
  .notifications {
    max-width: calc(100% - 2rem);
  }
}
</style>
