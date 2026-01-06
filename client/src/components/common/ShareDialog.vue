<template>
  <Modal
    :show="isActive"
    title="分享文件"
    mobile-compact
    @close="$emit('close')"
  >
    <div class="share-dialog">
      <div class="field">
        <label class="label">文件路径</label>
        <div class="control">
          <input class="input" type="text" :value="filePath" readonly />
        </div>
      </div>

      <div class="field">
        <label class="label">有效期</label>
        <div class="control">
          <div class="select is-fullwidth">
            <select v-model="selectedTtl">
              <option :value="3600">1 小时</option>
              <option :value="86400">1 天</option>
              <option :value="604800">7 天</option>
              <option :value="2592000">30 天</option>
            </select>
          </div>
        </div>
      </div>

      <div v-if="shareUrl" class="field">
        <label class="label">分享链接</label>
        <div class="control has-icons-right">
          <input
            class="input is-success"
            type="text"
            :value="shareUrl"
            readonly
            ref="urlInput"
          />
          <span class="icon is-small is-right">
            <IconCheck :size="16" />
          </span>
        </div>
        <p class="help is-success">链接有效期至 {{ expiresAtFormatted }}</p>
      </div>

      <div v-if="error" class="notification is-danger is-light">
        {{ error }}
      </div>
    </div>

    <template #footer>
      <div class="buttons is-right">
        <button class="button" @click="$emit('close')">关闭</button>
        <button
          v-if="!shareUrl"
          class="button is-primary"
          :class="{ 'is-loading': loading }"
          :disabled="loading"
          @click="createShare"
        >
          生成链接
        </button>
        <button v-else class="button is-success" @click="copyToClipboard">
          <span class="icon">
            <IconCheck v-if="copied" :size="16" />
            <IconCopy v-else :size="16" />
          </span>
          <span>{{ copied ? "已复制" : "复制链接" }}</span>
        </button>
      </div>
    </template>
  </Modal>
</template>

<script setup lang="ts">
import { ref, computed } from "vue";
import { IconCopy, IconCheck } from "@tabler/icons-vue";
import Modal from "../common/Modal.vue";
import { filesService } from "../../services/files.service";

const props = defineProps<{
  isActive: boolean;
  filePath: string;
  commit?: string;
}>();

const emit = defineEmits<{
  close: [];
}>();

const selectedTtl = ref(604800); // 默认 7 天
const loading = ref(false);
const error = ref("");
const shareUrl = ref("");
const expiresAt = ref("");
const copied = ref(false);
const urlInput = ref<HTMLInputElement | null>(null);

const expiresAtFormatted = computed(() => {
  if (!expiresAt.value) return "";
  const date = new Date(expiresAt.value);
  return date.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
});

async function createShare() {
  loading.value = true;
  error.value = "";
  shareUrl.value = "";

  try {
    const result = await filesService.createShareLink(props.filePath, {
      commit: props.commit,
      ttl: selectedTtl.value,
    });
    shareUrl.value = result.url;
    expiresAt.value = result.expiresAt;
  } catch (e: any) {
    error.value = e.message || "创建分享链接失败";
  } finally {
    loading.value = false;
  }
}

async function copyToClipboard() {
  if (!shareUrl.value) return;

  try {
    await navigator.clipboard.writeText(shareUrl.value);
    copied.value = true;
    setTimeout(() => {
      copied.value = false;
    }, 2000);
  } catch {
    // 回退方案
    if (urlInput.value) {
      urlInput.value.select();
      document.execCommand("copy");
      copied.value = true;
      setTimeout(() => {
        copied.value = false;
      }, 2000);
    }
  }
}
</script>

<style scoped>
.share-dialog {
  min-width: min(400px, 100%);
  max-width: 100%;
  box-sizing: border-box;
}

.share-dialog .field:not(:last-child) {
  margin-bottom: 1rem;
}

/* 手机端允许输入框内容换行/截断，避免横向滚动 */
.share-dialog .input {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

@media screen and (max-width: 480px) {
  .share-dialog {
    min-width: 0;
    width: 100%;
  }

  .share-dialog .input {
    font-size: 0.875rem;
  }
}
</style>
