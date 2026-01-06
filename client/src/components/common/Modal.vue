<template>
  <div class="modal" :class="{ 'is-active': show }">
    <div class="modal-background" @click="close"></div>
    <div class="modal-card" :class="{ 'is-mobile-compact': mobileCompact }">
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
withDefaults(
  defineProps<{
    show: boolean;
    title: string;
    mobileCompact?: boolean;
  }>(),
  {
    mobileCompact: false,
  },
);

const emit = defineEmits<{
  close: [];
}>();

function close() {
  emit("close");
}
</script>

<style scoped>
.modal {
  z-index: 100;
}

.modal-card {
  max-width: 90vw;
  max-height: 90vh;
  display: flex;
  flex-direction: column;
}

.modal-card-head {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 0;
}

.modal-card-title {
  min-width: 0;
  flex: 1 1 auto;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.modal-card-head .delete {
  flex: 0 0 auto;
  margin-left: auto;
}

.modal-card-body {
  overflow: auto;
  -webkit-overflow-scrolling: touch;
  overflow-x: hidden;
  max-width: 100%;
}

.modal-card-foot {
  justify-content: flex-end;
}

@media screen and (max-width: 768px) {
  .modal-card {
    width: 100%;
    margin: 0;
    max-height: 90vh;
  }

  /* 仅在需要时收紧移动端边距/内边距（例如“文件历史”） */
  .modal-card.is-mobile-compact .modal-card-head,
  .modal-card.is-mobile-compact .modal-card-body,
  .modal-card.is-mobile-compact .modal-card-foot {
    padding-left: 0.75rem;
    padding-right: 0.75rem;
  }

  .modal-card.is-mobile-compact .modal-card-head,
  .modal-card.is-mobile-compact .modal-card-foot {
    padding-top: 0.75rem;
    padding-bottom: 0.75rem;
  }

  .modal-card.is-mobile-compact .modal-card-body {
    padding-top: 0.75rem;
    padding-bottom: 0.75rem;
  }
}
</style>
