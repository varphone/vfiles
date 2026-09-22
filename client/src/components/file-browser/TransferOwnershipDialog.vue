<template>
  <Modal :show="show" title="转移所有权" mobile-compact @close="emit('close')">
    <div class="transfer-dialog">
      <p class="transfer-note">
        <IconInfoCircle :size="14" />
        <span>
          把所选条目交给另一个用户：<strong>版本历史会一起转移</strong>，转移后你不再拥有该条目。
        </span>
      </p>

      <div class="transfer-items">
        <span class="transfer-items-label">待转移</span>
        <span class="transfer-items-names" :title="itemsLabel">
          {{ itemsLabel }}
        </span>
      </div>

      <div class="transfer-field">
        <label class="transfer-label" for="transfer-target">接收用户</label>
        <div class="transfer-search">
          <IconSearch :size="15" class="transfer-search-icon" />
          <input
            id="transfer-target"
            v-model.trim="keyword"
            class="input is-small transfer-search-input"
            type="search"
            placeholder="搜索用户名"
            aria-label="搜索接收用户"
          />
        </div>

        <SkeletonList
          v-if="loading"
          variant="folders"
          :rows="3"
          label="加载用户列表"
        />

        <p v-else-if="error" class="transfer-error" role="alert">
          {{ error }}
        </p>

        <EmptyState
          v-else-if="filteredTargets.length === 0"
          :icon="IconUsers"
          compact
          title="没有其他可接收的用户"
          hint="需要至少两个可用账号才能转移所有权"
        />

        <ul v-else class="transfer-list" role="listbox">
          <li v-for="target in filteredTargets" :key="target.id">
            <button
              class="transfer-target"
              :class="{ 'is-selected': selectedId === target.id }"
              type="button"
              role="option"
              :aria-selected="selectedId === target.id ? 'true' : 'false'"
              @click="selectedId = target.id"
            >
              <span class="transfer-avatar" aria-hidden="true">
                {{ target.username.slice(0, 1).toUpperCase() }}
              </span>
              <span class="transfer-username">{{ target.username }}</span>
              <IconCheck v-if="selectedId === target.id" :size="16" />
            </button>
          </li>
        </ul>
      </div>

      <div class="transfer-field">
        <label class="transfer-label" for="transfer-message">
          备注（可选，写入双方版本历史）
        </label>
        <input
          id="transfer-message"
          v-model.trim="message"
          class="input is-small"
          type="text"
          maxlength="200"
          placeholder="例如：项目交接给 @同事"
        />
      </div>
    </div>

    <template #footer>
      <button class="vf-ghost-button" @click="emit('close')">取消</button>
      <button
        class="vf-ghost-button is-primary"
        type="button"
        :disabled="!selectedId || submitting"
        @click="confirm"
      >
        <IconArrowsDiff :size="16" />
        <span>{{ submitting ? "转移中…" : "转移所有权" }}</span>
      </button>
    </template>
  </Modal>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  IconArrowsDiff,
  IconCheck,
  IconInfoCircle,
  IconSearch,
  IconUsers,
} from "@tabler/icons-vue";
import Modal from "../common/Modal.vue";
import EmptyState from "../common/EmptyState.vue";
import SkeletonList from "../common/SkeletonList.vue";
import { confirmDialog } from "../../composables/dialog";
import { filesService } from "../../services/files.service";
import type { FileInfo, TransferTarget } from "../../types";

/**
 * 所有权转移对话框。
 *
 * 只负责选择接收用户与确认，实际转移由父组件调用服务完成（便于刷新列表与提示）。
 */
const props = withDefaults(
  defineProps<{
    show: boolean;
    items?: Pick<FileInfo, "name" | "path">[];
  }>(),
  { items: () => [] },
);

const emit = defineEmits<{
  (e: "close"): void;
  (e: "transfer", target: TransferTarget, message: string): void;
}>();

const targets = ref<TransferTarget[]>([]);
const loading = ref(false);
const error = ref("");
const keyword = ref("");
const selectedId = ref("");
const message = ref("");
const submitting = ref(false);

const filteredTargets = computed(() => {
  const query = keyword.value.toLowerCase();
  if (!query) return targets.value;
  return targets.value.filter((target) =>
    target.username.toLowerCase().includes(query),
  );
});

const itemsLabel = computed(() => {
  const names = props.items
    .slice(0, 3)
    .map((item) => item.name)
    .join("、");
  return props.items.length > 3
    ? `${names}… 共 ${props.items.length} 项`
    : names;
});

const selected = computed(() =>
  targets.value.find((target) => target.id === selectedId.value),
);

async function loadTargets() {
  loading.value = true;
  error.value = "";
  try {
    targets.value = await filesService.listTransferTargets();
  } catch (e) {
    targets.value = [];
    error.value = e instanceof Error ? e.message : "加载用户列表失败";
  } finally {
    loading.value = false;
  }
}

watch(
  () => props.show,
  (show) => {
    if (!show) return;
    keyword.value = "";
    selectedId.value = "";
    message.value = "";
    submitting.value = false;
    void loadTargets();
  },
  { immediate: true },
);

/** 转移是不可逆的（对方拥有、你失去），因此再做一次确认。 */
async function confirm() {
  const target = selected.value;
  if (!target) return;

  const ok = await confirmDialog({
    title: "转移所有权",
    message: `确定把 ${props.items.length} 个条目转移给 ${target.username} 吗？\n（版本历史会一起转移，之后你不再拥有这些条目）`,
    confirmText: "转移",
  });
  if (!ok) return;

  submitting.value = true;
  emit("transfer", target, message.value);
}

/** 由父组件在提交完成后调用，恢复按钮状态。 */
defineExpose({
  finish: () => {
    submitting.value = false;
  },
});
</script>

<style scoped>
.transfer-dialog {
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
  min-width: min(460px, 100%);
}

.transfer-note {
  display: flex;
  align-items: flex-start;
  gap: 0.35rem;
  margin: 0;
  padding: 0.45rem 0.55rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface-sunken);
  color: var(--vf-text-muted);
  font-size: 0.78rem;
  line-height: 1.5;
}

.transfer-note strong {
  color: var(--vf-text-strong);
}

.transfer-items {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  min-width: 0;
}

.transfer-items-label {
  flex: 0 0 auto;
  color: var(--vf-text-subtle);
  font-size: 0.76rem;
}

.transfer-items-names {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--vf-text-strong);
  font-size: 0.84rem;
  font-weight: 600;
}

.transfer-field {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.transfer-label {
  color: var(--vf-text-muted);
  font-size: 0.78rem;
  font-weight: 600;
}

.transfer-search {
  position: relative;
  display: flex;
  align-items: center;
}

.transfer-search-icon {
  position: absolute;
  left: 0.5rem;
  z-index: 1;
  color: var(--vf-text-subtle);
  pointer-events: none;
}

.transfer-search-input {
  padding-left: 1.75rem;
  font-size: 0.8rem;
}

.transfer-list {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0.2rem;
  max-height: 12rem;
  overflow-y: auto;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  list-style: none;
}

.transfer-target {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  width: 100%;
  padding: 0.4rem 0.5rem;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: var(--vf-text);
  font-size: 0.84rem;
  text-align: left;
  cursor: pointer;
}

.transfer-target:hover {
  background: var(--vf-surface-hover);
}

.transfer-target.is-selected {
  background: var(--vf-accent-soft);
  color: var(--vf-accent-text);
  font-weight: 600;
}

.transfer-avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: 0 0 auto;
  width: 1.6rem;
  height: 1.6rem;
  border-radius: 50%;
  background: var(--vf-accent-soft);
  color: var(--vf-accent-text);
  font-size: 0.76rem;
  font-weight: 600;
}

.transfer-username {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.transfer-error {
  margin: 0;
  color: var(--vf-danger-text);
  font-size: 0.8rem;
}
</style>
