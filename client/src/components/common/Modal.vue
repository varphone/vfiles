<template>
  <div class="modal" :class="{ 'is-active': show }">
    <div class="modal-background" @click="close"></div>
    <div class="modal-card">
      <header class="modal-card-head">
        <p class="modal-card-title">{{ title }}</p>
        <button class="delete" aria-label="close" @click="close"></button>
      </header>
      <section class="modal-card-body">
        <slot></slot>
      </section>
      <footer class="modal-card-foot" v-if="$slots.footer">
        <slot name="footer"></slot>
      </footer>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  show: boolean;
  title: string;
}>();

const emit = defineEmits<{
  close: [];
}>();

function close() {
  emit('close');
}
</script>

<style scoped>
.modal-card {
  max-width: 90vw;
}

.modal-card-body {
  -webkit-overflow-scrolling: touch;
}

@media screen and (max-width: 768px) {
  .modal-card {
    width: 100%;
    margin: 0;
    height: 100%;
    max-height: 100vh;
  }
}
</style>
