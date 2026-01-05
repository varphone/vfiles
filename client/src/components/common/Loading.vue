<template>
  <div class="loading-overlay" v-if="show">
    <div class="loading-spinner">
      <div class="spinner"></div>
      <p v-if="message" class="has-text-white mt-3">{{ message }}</p>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  show: boolean;
  message?: string;
}>();
</script>

<style scoped>
.loading-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.7);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
}

.loading-spinner {
  text-align: center;
}

.spinner {
  width: 50px;
  height: 50px;
  border: 4px solid rgba(255, 255, 255, 0.3);
  border-top-color: white;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  margin: 0 auto;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
