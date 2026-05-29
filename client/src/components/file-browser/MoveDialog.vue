<template>
  <Modal
    :show="isActive"
    :title="dialogTitle"
    mobile-compact
    @close="emit('close')"
  >
    <div class="move-dialog">
      <div class="move-dialog-summary">
        <div class="move-dialog-summary-title">
          待移动 {{ items.length }} 项
        </div>
        <div class="move-dialog-summary-list">
          <span
            v-for="item in previewItems"
            :key="item.path"
            class="move-dialog-chip"
          >
            {{ item.name }}
          </span>
          <span v-if="hiddenItemCount > 0" class="move-dialog-chip is-muted">
            还有 {{ hiddenItemCount }} 项
          </span>
        </div>
      </div>

      <div class="move-dialog-toolbar">
        <button class="button is-small is-light" @click="goRoot" :disabled="loading">
          根目录
        </button>
        <button
          class="button is-small is-light"
          @click="goUp"
          :disabled="loading || !browserPath"
        >
          上一级
        </button>
      </div>

      <div class="move-dialog-target">
        <div class="move-dialog-target-label">当前目标目录</div>
        <div class="move-dialog-target-value">{{ currentPathLabel }}</div>
      </div>

      <div v-if="breadcrumbs.length" class="move-dialog-breadcrumbs">
        <button class="move-dialog-crumb" @click="goRoot">根目录</button>
        <template v-for="crumb in breadcrumbs" :key="crumb.path">
          <span class="move-dialog-crumb-sep">/</span>
          <button class="move-dialog-crumb" @click="openDirectory(crumb.path)">
            {{ crumb.name }}
          </button>
        </template>
      </div>

      <p class="move-dialog-hint">只显示目录；不可作为目标的目录会直接置灰。</p>

      <div v-if="error" class="notification is-danger is-light">
        {{ error }}
      </div>

      <div
        class="move-dialog-target-status"
        :class="validationMessages.length ? 'is-warning' : 'is-ready'"
      >
        <template v-if="validationMessages.length">
          <div class="move-dialog-target-status-title">当前目录暂不可作为目标</div>
          <ul class="move-dialog-target-status-list">
            <li v-for="message in validationMessages" :key="message">
              {{ message }}
            </li>
          </ul>
        </template>
        <template v-else>
          <div class="move-dialog-target-status-title">当前目录可作为目标</div>
          <p class="move-dialog-target-status-text">
            确认后会把所选项目移动到这里。
          </p>
        </template>
      </div>

      <div class="move-dialog-browser">
        <div v-if="loading" class="move-dialog-state has-text-grey">加载目录中...</div>
        <div v-else-if="directories.length === 0" class="move-dialog-state has-text-grey">
          当前目录下没有可继续进入的子目录。
        </div>
        <button
          v-for="directory in directories"
          :key="directory.path"
          class="move-dialog-directory"
          :class="{ 'is-disabled': isDirectoryDisabled(directory.path) }"
          :disabled="isDirectoryDisabled(directory.path)"
          @click="openDirectory(directory.path)"
        >
          <span class="move-dialog-directory-name">{{ directory.name }}</span>
          <span class="move-dialog-directory-path">/{{ directory.path }}</span>
          <span
            v-if="directoryDisabledReason(directory.path)"
            class="move-dialog-directory-note"
          >
            {{ directoryDisabledReason(directory.path) }}
          </span>
        </button>
      </div>
    </div>

    <template #footer>
      <button class="button" @click="emit('close')">取消</button>
      <button
        class="button is-primary"
        :class="{ 'is-loading': confirmLoading }"
        :disabled="!canConfirm"
        @click="emit('confirm', browserPath)"
      >
        移动到当前目录
      </button>
    </template>
  </Modal>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Modal from "../common/Modal.vue";
import { filesService } from "../../services/files.service";
import type { FileInfo } from "../../types";

type MoveDialogItem = Pick<FileInfo, "name" | "path" | "kind">;

type BreadcrumbItem = {
  name: string;
  path: string;
};

const props = withDefaults(
  defineProps<{
    isActive: boolean;
    items: MoveDialogItem[];
    initialPath?: string;
    confirmLoading?: boolean;
  }>(),
  {
    initialPath: "",
    confirmLoading: false,
  },
);

const emit = defineEmits<{
  close: [];
  confirm: [targetDir: string];
}>();

const browserPath = ref("");
const browserEntries = ref<FileInfo[]>([]);
const loading = ref(false);
const error = ref("");

const dialogTitle = computed(() => {
  if (props.items.length === 1) {
    return `移动: ${props.items[0]?.name ?? "项目"}`;
  }
  return `移动 ${props.items.length} 个项目`;
});

const previewItems = computed(() => props.items.slice(0, 4));
const hiddenItemCount = computed(() => Math.max(0, props.items.length - previewItems.value.length));
const currentPathLabel = computed(() => (browserPath.value ? `/${browserPath.value}` : "/"));
const directories = computed(() => {
  return browserEntries.value
    .filter((entry) => entry.kind === "directory")
    .sort((left, right) => left.name.localeCompare(right.name, "zh-CN"));
});
const breadcrumbs = computed<BreadcrumbItem[]>(() => {
  const parts = browserPath.value.split("/").filter(Boolean);
  const items: BreadcrumbItem[] = [];
  let current = "";

  for (const part of parts) {
    current = current ? `${current}/${part}` : part;
    items.push({ name: part, path: current });
  }

  return items;
});

const validationMessages = computed(() => {
  if (loading.value) return [] as string[];

  const issues = new Set<string>();
  const usedTargets = new Set<string>();
  const existingPaths = new Set(browserEntries.value.map((entry) => entry.path));

  for (const item of props.items) {
    if (
      item.kind === "directory" &&
      (browserPath.value === item.path || browserPath.value.startsWith(`${item.path}/`))
    ) {
      issues.add(`“${item.name}”不能移动到自身或其子目录`);
      continue;
    }

    const targetPath = buildTargetPath(item.name);
    if (targetPath === item.path) {
      issues.add(`“${item.name}”已经在当前目录`);
      continue;
    }

    if (usedTargets.has(targetPath)) {
      issues.add(`移动后会产生重名项：${item.name}`);
      continue;
    }

    if (existingPaths.has(targetPath)) {
      issues.add(`目标目录已存在同名项目：${item.name}`);
      continue;
    }

    usedTargets.add(targetPath);
  }

  return Array.from(issues);
});

const canConfirm = computed(() => {
  return !loading.value && !props.confirmLoading && validationMessages.value.length === 0;
});

watch(
  () => [props.isActive, props.initialPath] as const,
  async ([isActive, initialPath]) => {
    if (!isActive) return;
    browserPath.value = normalizePath(initialPath ?? "");
    await loadDirectories(browserPath.value);
  },
  { immediate: true },
);

function normalizePath(rawPath: string): string {
  return rawPath
    .trim()
    .replace(/\\/g, "/")
    .replace(/^\/+/, "")
    .replace(/\/+$/, "");
}

async function loadDirectories(path: string) {
  loading.value = true;
  error.value = "";

  try {
    browserEntries.value = await filesService.getFiles(path);
  } catch (err) {
    browserEntries.value = [];
    error.value = err instanceof Error ? err.message : "目录加载失败";
  } finally {
    loading.value = false;
  }
}

function buildTargetPath(name: string): string {
  return browserPath.value ? `${browserPath.value}/${name}` : name;
}

function isDirectoryDisabled(path: string): boolean {
  return props.items.some(
    (item) => item.kind === "directory" && (path === item.path || path.startsWith(`${item.path}/`)),
  );
}

function directoryDisabledReason(path: string): string {
  if (!isDirectoryDisabled(path)) return "";
  return "待移动目录及其子目录不可作为目标";
}

async function openDirectory(path: string) {
  if (isDirectoryDisabled(path)) return;
  const nextPath = normalizePath(path);
  browserPath.value = nextPath;
  await loadDirectories(nextPath);
}

async function goRoot() {
  await openDirectory("");
}

async function goUp() {
  if (!browserPath.value) return;
  const parts = browserPath.value.split("/").filter(Boolean);
  parts.pop();
  await openDirectory(parts.join("/"));
}
</script>

<style scoped>
.move-dialog {
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
  min-width: min(560px, 100%);
}

.move-dialog-summary {
  padding: 0.8rem 0.9rem;
  border-radius: 14px;
  background: rgba(241, 245, 251, 0.88);
  border: 1px solid rgba(214, 223, 235, 0.92);
}

.move-dialog-summary-title {
  margin-bottom: 0.55rem;
  font-size: 0.78rem;
  font-weight: 700;
  letter-spacing: 0.04em;
  color: #607284;
  text-transform: uppercase;
}

.move-dialog-summary-list {
  display: flex;
  flex-wrap: wrap;
  gap: 0.45rem;
}

.move-dialog-chip {
  display: inline-flex;
  align-items: center;
  max-width: 100%;
  padding: 0.32rem 0.6rem;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.95);
  border: 1px solid rgba(214, 223, 235, 0.95);
  color: #2a4058;
  font-size: 0.82rem;
  line-height: 1.2;
}

.move-dialog-chip.is-muted {
  color: #6d7f95;
}

.move-dialog-toolbar {
  display: flex;
  gap: 0.55rem;
  flex-wrap: wrap;
}

.move-dialog-target {
  padding: 0.75rem 0.9rem;
  border-radius: 14px;
  background: rgba(255, 255, 255, 0.92);
  border: 1px solid rgba(214, 223, 235, 0.92);
}

.move-dialog-target-label {
  font-size: 0.74rem;
  font-weight: 700;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: #71859d;
}

.move-dialog-target-value {
  margin-top: 0.32rem;
  color: #203247;
  font-weight: 600;
  word-break: break-word;
}

.move-dialog-breadcrumbs {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.2rem;
}

.move-dialog-crumb {
  padding: 0;
  border: none;
  background: transparent;
  color: #2a66b8;
  cursor: pointer;
  font-size: 0.88rem;
}

.move-dialog-crumb:hover {
  color: #194c93;
  text-decoration: underline;
}

.move-dialog-crumb-sep {
  color: #8da0b6;
}

.move-dialog-hint {
  margin: -0.1rem 0 0;
  color: #6d7f95;
  font-size: 0.82rem;
}

.move-dialog-target-status {
  padding: 0.78rem 0.9rem;
  border-radius: 14px;
  border: 1px solid rgba(214, 223, 235, 0.92);
}

.move-dialog-target-status.is-ready {
  background: rgba(34, 197, 94, 0.1);
  border-color: rgba(34, 197, 94, 0.22);
}

.move-dialog-target-status.is-warning {
  background: rgba(245, 158, 11, 0.12);
  border-color: rgba(245, 158, 11, 0.26);
}

.move-dialog-target-status-title {
  color: #21364d;
  font-weight: 700;
}

.move-dialog-target-status-text {
  margin: 0.32rem 0 0;
  color: #5f7084;
  font-size: 0.84rem;
}

.move-dialog-target-status-list {
  margin: 0.35rem 0 0;
  padding-left: 1.15rem;
  color: #7c4a03;
  font-size: 0.84rem;
}

.move-dialog-browser {
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
  min-height: 240px;
  max-height: min(52vh, 420px);
  overflow: auto;
  padding: 0.35rem;
  border-radius: 16px;
  background: rgba(248, 250, 253, 0.92);
  border: 1px solid rgba(214, 223, 235, 0.92);
}

.move-dialog-directory {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 0.18rem;
  width: 100%;
  padding: 0.8rem 0.9rem;
  border: 1px solid rgba(214, 223, 235, 0.92);
  border-radius: 14px;
  background: rgba(255, 255, 255, 0.96);
  cursor: pointer;
  text-align: left;
}

.move-dialog-directory:hover {
  border-color: rgba(81, 132, 208, 0.55);
  background: rgba(240, 246, 255, 0.96);
}

.move-dialog-directory.is-disabled {
  cursor: not-allowed;
  opacity: 0.62;
  background: rgba(243, 244, 246, 0.96);
}

.move-dialog-directory.is-disabled:hover {
  border-color: rgba(214, 223, 235, 0.92);
  background: rgba(243, 244, 246, 0.96);
}

.move-dialog-directory-name {
  color: #21364d;
  font-weight: 600;
}

.move-dialog-directory-path {
  color: #6d7f95;
  font-size: 0.8rem;
}

.move-dialog-directory-note {
  margin-top: 0.15rem;
  color: #8b5e12;
  font-size: 0.76rem;
}

.move-dialog-state {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 200px;
  text-align: center;
  padding: 1rem;
}

@media screen and (max-width: 768px) {
  .move-dialog {
    min-width: 0;
  }

  .move-dialog-browser {
    max-height: 46vh;
  }
}
</style>
