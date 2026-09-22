<template>
  <Modal
    :show="isActive"
    :title="dialogTitle"
    mobile-compact
    @close="emit('close')"
  >
    <div class="share-dialog">
      <!-- 被分享的条目：静态展示一行，而不是让用户面对一个只读的路径输入框 -->
      <div class="share-item">
        <span class="share-item-icon" aria-hidden="true">
          <FileTypeIcon v-if="item" :file="item" :size="20" />
          <IconFile v-else :size="20" />
        </span>
        <div class="share-item-titles">
          <p class="share-item-name" :title="filePath">{{ displayName }}</p>
          <p class="share-item-path" :title="filePath">
            {{ filePath ? `/${filePath}` : "/" }}
          </p>
        </div>
      </div>

      <div v-if="!shareUrl" class="share-field">
        <label class="share-label" for="share-ttl">链接有效期</label>
        <div class="select is-fullwidth">
          <select id="share-ttl" v-model="selectedTtl">
            <option
              v-for="option in ttlOptions"
              :key="option.value"
              :value="option.value"
            >
              {{ option.label }}
            </option>
          </select>
        </div>
        <p class="share-hint">
          {{
            isPermanent
              ? "永久链接不会自动失效，需要手动停止分享。"
              : "到期后链接自动失效；随时可以在这里停止分享。"
          }}
        </p>
      </div>

      <!-- 生成结果：链接卡片 + 复制/打开/停止 -->
      <div v-else class="share-result">
        <label class="share-label" for="share-url">分享链接</label>
        <div class="share-link-row">
          <input
            id="share-url"
            ref="linkInput"
            class="input share-link-input"
            type="text"
            :value="shareUrl"
            readonly
            @focus="selectLink"
          />
          <button
            class="vf-ghost-button is-primary share-copy"
            type="button"
            @click="copyToClipboard"
          >
            <IconCheck v-if="copied" :size="16" />
            <IconCopy v-else :size="16" />
            <span>{{ copied ? "已复制" : "复制" }}</span>
          </button>
        </div>

        <div class="share-result-meta">
          <span class="share-chip">
            <IconClock :size="14" />
            <span v-if="expiresAtFormatted"
              >有效期至 {{ expiresAtFormatted }}</span
            >
            <span v-else>永久有效</span>
          </span>
          <button
            class="vf-ghost-button share-open"
            type="button"
            @click="openLink"
          >
            <IconExternalLink :size="16" />
            <span>打开链接</span>
          </button>
          <button
            class="vf-ghost-button is-danger share-stop"
            type="button"
            :disabled="stopping"
            @click="stopSharing"
          >
            <IconTrash :size="16" />
            <span>{{ stopping ? "停止中…" : "停止分享" }}</span>
          </button>
        </div>

        <p v-if="copied" class="share-copy-feedback">
          <IconCheck :size="16" />
          <span>链接已复制到剪贴板，可以直接发送给对方</span>
        </p>
      </div>

      <p v-if="error" class="share-error" role="alert">
        <IconAlertCircle :size="16" class="share-error-icon" />
        <span>{{ error }}</span>
      </p>
    </div>

    <template #footer>
      <button class="vf-ghost-button" @click="emit('close')">关闭</button>
      <button
        v-if="!shareUrl"
        class="vf-ghost-button is-primary"
        :class="{ 'is-loading': loading }"
        :disabled="loading || !filePath"
        @click="createShare"
      >
        <IconLink :size="16" />
        <span>生成链接</span>
      </button>
    </template>
  </Modal>
</template>

<script setup lang="ts">
import { formatDate } from "../../utils/filePresentation";
import { computed, nextTick, ref, watch } from "vue";
import {
  IconAlertCircle,
  IconCheck,
  IconClock,
  IconCopy,
  IconExternalLink,
  IconFile,
  IconLink,
  IconTrash,
} from "@tabler/icons-vue";
import Modal from "../common/Modal.vue";
import FileTypeIcon from "../file-browser/FileTypeIcon.vue";
import { filesService } from "../../services/files.service";
import { copyText } from "../../utils/clipboard";
import type { FileInfo } from "../../types";

const props = defineProps<{
  isActive: boolean;
  filePath: string;
  commit?: string;
  /** 可选：带完整信息时显示真实的类型图标 */
  file?: FileInfo;
}>();

const emit = defineEmits<{
  close: [];
}>();

/** 链接有效期选项：`value` 为秒数，0 表示永久有效。 */
const ttlOptions = [
  { value: 3600, label: "1 小时" },
  { value: 86400, label: "1 天" },
  { value: 604800, label: "7 天" },
  { value: 2592000, label: "30 天" },
  { value: 31536000, label: "1 年" },
  { value: 0, label: "永久" },
] as const;

const selectedTtl = ref<number>(604800); // 默认 7 天
const isPermanent = computed(() => selectedTtl.value === 0);
const loading = ref(false);
const stopping = ref(false);
const error = ref("");
const shareUrl = ref("");
const shareCode = ref("");
const expiresAt = ref("");
const copied = ref(false);
const linkInput = ref<HTMLInputElement | null>(null);

const displayName = computed(
  () => props.file?.name || props.filePath.split("/").pop() || "文件",
);
const item = computed(() => props.file);
const dialogTitle = computed(() => `分享「${displayName.value}」`);

const expiresAtFormatted = computed(() => {
  if (!expiresAt.value) return "";
  const date = new Date(expiresAt.value);
  if (Number.isNaN(date.getTime())) return "";
  return formatDate(date.toISOString());
});

// 关闭后重置：下次打开是干净的表单，避免沿用上一次的链接
watch(
  () => props.isActive,
  (active) => {
    if (active) return;
    shareUrl.value = "";
    shareCode.value = "";
    expiresAt.value = "";
    error.value = "";
    copied.value = false;
  },
);

async function createShare() {
  loading.value = true;
  error.value = "";
  shareUrl.value = "";

  try {
    const result = await filesService.createShareLink(props.filePath, {
      commit: props.commit,
      // 0 表示永久：服务端收到没有 expires_at 的请求即视为不过期
      ttl: selectedTtl.value,
    });
    shareUrl.value = result.url;
    shareCode.value = result.code;
    expiresAt.value = result.expiresAt;
    await nextTick();
    linkInput.value?.focus();
  } catch (e) {
    error.value = e instanceof Error ? e.message : "创建分享链接失败";
  } finally {
    loading.value = false;
  }
}

/** 聚焦链接输入框时全选，便于手动复制。 */
function selectLink() {
  linkInput.value?.select();
}

async function copyToClipboard() {
  if (!shareUrl.value) return;

  const ok = await copyText(shareUrl.value);
  if (!ok) {
    error.value = "复制失败，请手动选择链接复制";
    return;
  }

  error.value = "";
  copied.value = true;
  window.setTimeout(() => {
    copied.value = false;
  }, 2000);
}

function openLink() {
  if (!shareUrl.value || typeof window === "undefined") return;
  window.open(shareUrl.value, "_blank", "noopener,noreferrer");
}

/** 停止分享：删除链接并回到表单态。 */
async function stopSharing() {
  if (!shareCode.value || stopping.value) return;
  stopping.value = true;
  error.value = "";

  try {
    await filesService.disableShare(shareCode.value);
    shareUrl.value = "";
    shareCode.value = "";
    expiresAt.value = "";
    copied.value = false;
  } catch (e) {
    error.value = e instanceof Error ? e.message : "停止分享失败";
  } finally {
    stopping.value = false;
  }
}
</script>

<style scoped>
.share-dialog {
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
  min-width: min(440px, 100%);
  max-width: 100%;
  box-sizing: border-box;
}

/* 被分享条目：图标 + 名称 + 路径 */
.share-item {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  min-width: 0;
  padding: 0.55rem 0.65rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface-sunken);
}

.share-item-icon {
  display: inline-flex;
  flex: 0 0 auto;
  color: var(--vf-accent-text);
}

.share-item-titles {
  min-width: 0;
}

.share-item-name {
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--vf-text-strong);
}

.share-item-path {
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.75rem;
  color: var(--vf-text-subtle);
}

.share-field {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.share-label {
  font-size: 0.8rem;
  font-weight: 600;
  color: var(--vf-text-muted);
}

.share-hint {
  margin: 0;
  font-size: 0.8rem;
  color: var(--vf-text-subtle);
}

/* 链接行：输入框 + 复制按钮 */
.share-link-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  min-width: 0;
}

.share-link-input {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.875rem;
}

.share-copy {
  flex: 0 0 auto;
}

.share-result {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.share-result-meta {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  flex-wrap: wrap;
}

.share-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  padding: 0.2rem 0.5rem;
  border-radius: var(--vf-radius-pill);
  background: var(--vf-surface-sunken);
  color: var(--vf-text-muted);
  font-size: 0.8rem;
}

.share-open,
.share-stop {
  min-height: 1.85rem;
  padding: 0 0.55rem;
  font-size: 0.8rem;
}

.share-copy-feedback {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  margin: 0;
  padding: 0.45rem 0.55rem;
  border-radius: var(--vf-radius-sm);
  background: var(--vf-success-soft);
  color: var(--vf-success-text);
  font-size: 0.8rem;
  font-weight: 600;
}

.share-error {
  display: flex;
  align-items: flex-start;
  gap: 0.35rem;
  margin: 0;
  padding: 0.45rem 0.55rem;
  border: 1px solid var(--vf-danger-soft);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-danger-soft);
  color: var(--vf-danger-text);
  font-size: 0.8rem;
  line-height: 1.5;
}

@media screen and (max-width: 480px) {
  .share-dialog {
    min-width: 0;
    width: 100%;
  }
}
</style>
