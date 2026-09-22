<template>
  <Modal
    :show="isActive"
    :title="dialogTitle"
    mobile-compact
    @close="emit('close')"
  >
    <div class="move-dialog">
      <!-- 待移动条目：紧凑一行，避免把已知信息做成大盒子 -->
      <div class="move-dialog-items">
        <span class="move-dialog-items-icon" aria-hidden="true">
          <FileTypeIcon v-if="singleItem" :file="singleItem" :size="18" />
          <IconFolders v-else :size="18" />
        </span>
        <span class="move-dialog-items-name" :title="itemsLabel">
          {{ itemsLabel }}
        </span>
        <span v-if="hiddenItemCount > 0" class="move-dialog-items-more">
          共 {{ items.length }} 项
        </span>
      </div>

      <!-- 目标选择器：面包屑 + 子目录列表 -->
      <div class="move-dialog-picker">
        <div class="move-dialog-path">
          <button
            class="vf-icon-button move-dialog-up"
            type="button"
            title="上一级"
            aria-label="上一级"
            :disabled="loading || !browserPath"
            @click="goUp"
          >
            <IconArrowUp :size="16" />
          </button>

          <nav class="move-dialog-crumbs" aria-label="目标目录">
            <button
              class="move-dialog-crumb"
              :class="{ 'is-current': !browserPath }"
              type="button"
              @click="goRoot"
            >
              <IconHome :size="14" />
              <span>根目录</span>
            </button>
            <template v-for="crumb in breadcrumbs" :key="crumb.path">
              <span class="move-dialog-crumb-sep" aria-hidden="true">/</span>
              <button
                class="move-dialog-crumb"
                :class="{ 'is-current': crumb.path === browserPath }"
                type="button"
                @click="openDirectory(crumb.path)"
              >
                {{ crumb.name }}
              </button>
            </template>
          </nav>
        </div>

        <div class="move-dialog-browser">
          <SkeletonList
            row-height="38px"
            v-if="loading"
            variant="folders"
            :rows="4"
            label="加载目录"
          />

          <EmptyState
            v-else-if="error"
            :icon="IconAlertCircle"
            tone="error"
            compact
            title="目录加载失败"
            :hint="error"
          >
            <template #actions>
              <button
                class="vf-ghost-button"
                @click="loadDirectories(browserPath)"
              >
                <span>重试</span>
              </button>
            </template>
          </EmptyState>

          <EmptyState
            v-else-if="directories.length === 0"
            :icon="IconFolderOff"
            compact
            title="此处没有子文件夹"
            hint="可以直接移动到当前目录"
          />

          <ul v-else class="move-dialog-list">
            <li v-for="directory in directories" :key="directory.path">
              <button
                class="move-dialog-row"
                type="button"
                :class="{ 'is-disabled': isDirectoryDisabled(directory.path) }"
                :disabled="isDirectoryDisabled(directory.path)"
                :title="
                  directoryDisabledReason(directory.path) || directory.path
                "
                @click="openDirectory(directory.path)"
              >
                <IconFolder :size="18" class="move-dialog-row-icon" />
                <span class="move-dialog-row-name">{{ directory.name }}</span>
                <span
                  v-if="directoryDisabledReason(directory.path)"
                  class="move-dialog-row-note"
                >
                  待移动目录
                </span>
                <IconChevronRight
                  v-else
                  :size="16"
                  class="move-dialog-row-chevron"
                />
              </button>
            </li>
          </ul>
        </div>

        <!-- 仅在当前目录不可用时提示一行，而不是常驻状态卡片 -->
        <p v-if="validationMessages.length" class="move-dialog-warning">
          <IconAlertTriangle :size="14" class="move-dialog-warning-icon" />
          <span>{{ validationMessages.join("；") }}</span>
        </p>
      </div>
    </div>

    <template #footer>
      <button class="vf-ghost-button" @click="emit('close')">取消</button>
      <button
        class="vf-ghost-button is-primary"
        :class="{ 'is-loading': confirmLoading }"
        :disabled="!canConfirm"
        :title="
          validationMessages.length
            ? validationMessages.join('；')
            : `移动到${targetLabel}`
        "
        @click="emit('confirm', browserPath)"
      >
        <span>{{ confirmLabel }}</span>
      </button>
    </template>
  </Modal>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  IconAlertCircle,
  IconAlertTriangle,
  IconArrowUp,
  IconChevronRight,
  IconFolder,
  IconFolderOff,
  IconFolders,
  IconHome,
} from "@tabler/icons-vue";
import Modal from "../common/Modal.vue";
import EmptyState from "../common/EmptyState.vue";
import SkeletonList from "../common/SkeletonList.vue";
import FileTypeIcon from "./FileTypeIcon.vue";
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

const singleItem = computed(() =>
  props.items.length === 1 ? props.items[0] : undefined,
);
const previewItems = computed(() => props.items.slice(0, 3));
const hiddenItemCount = computed(() =>
  Math.max(0, props.items.length - previewItems.value.length),
);
const itemsLabel = computed(() => {
  const names = previewItems.value.map((item) => item.name).join("、");
  return hiddenItemCount.value > 0 ? `${names}…` : names;
});

const targetLabel = computed(() =>
  breadcrumbs.value.length
    ? `「${breadcrumbs.value[breadcrumbs.value.length - 1]?.name}」`
    : "根目录",
);
const confirmLabel = computed(() =>
  validationMessages.value.length
    ? "移动到当前目录"
    : `移动到${targetLabel.value}`,
);

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
  const existingPaths = new Set(
    browserEntries.value.map((entry) => entry.path),
  );

  for (const item of props.items) {
    if (
      item.kind === "directory" &&
      (browserPath.value === item.path ||
        browserPath.value.startsWith(`${item.path}/`))
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
  return (
    !loading.value &&
    !props.confirmLoading &&
    validationMessages.value.length === 0
  );
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
    (item) =>
      item.kind === "directory" &&
      (path === item.path || path.startsWith(`${item.path}/`)),
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
  gap: 0.85rem;
  min-width: min(560px, 100%);
}

/* 待移动条目：一行摘要 */
.move-dialog-items {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  min-width: 0;
  padding: 0.5rem 0.65rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface-sunken);
}

.move-dialog-items-icon {
  display: inline-flex;
  flex: 0 0 auto;
  color: var(--vf-accent-text);
}

.move-dialog-items-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.84rem;
  font-weight: 600;
  color: var(--vf-text-strong);
}

.move-dialog-items-more {
  flex: 0 0 auto;
  color: var(--vf-text-subtle);
  font-size: 0.76rem;
}

/* 目标选择器：路径 + 列表合成一个面板 */
.move-dialog-picker {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  min-height: 0;
}

.move-dialog-path {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  min-width: 0;
  padding: 0.35rem 0.45rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface);
}

.move-dialog-up {
  flex: 0 0 auto;
}

.move-dialog-crumbs {
  display: flex;
  align-items: center;
  gap: 0.15rem;
  min-width: 0;
  overflow-x: auto;
  white-space: nowrap;
}

.move-dialog-crumb {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  flex: 0 0 auto;
  padding: 0.15rem 0.35rem;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: var(--vf-text-muted);
  font-size: 0.82rem;
  cursor: pointer;
}

.move-dialog-crumb:hover {
  background: var(--vf-surface-hover);
  color: var(--vf-text-strong);
}

.move-dialog-crumb.is-current {
  color: var(--vf-text-strong);
  font-weight: 600;
}

.move-dialog-crumb-sep {
  flex: 0 0 auto;
  color: var(--vf-border);
}

.move-dialog-browser {
  min-height: 12rem;
  max-height: min(46vh, 22rem);
  overflow-y: auto;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface);
}

.move-dialog-list {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0.25rem;
  list-style: none;
}

.move-dialog-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  width: 100%;
  padding: 0.5rem 0.55rem;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: var(--vf-text);
  font-size: 0.86rem;
  text-align: left;
  cursor: pointer;
}

.move-dialog-row:hover:not(:disabled) {
  background: var(--vf-surface-hover);
}

.move-dialog-row.is-disabled,
.move-dialog-row:disabled {
  color: var(--vf-text-subtle);
  cursor: default;
}

.move-dialog-row-icon {
  flex: 0 0 auto;
  color: var(--vf-accent-text);
}

.move-dialog-row.is-disabled .move-dialog-row-icon {
  color: var(--vf-text-subtle);
}

.move-dialog-row-name {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.move-dialog-row-note {
  flex: 0 0 auto;
  color: var(--vf-text-subtle);
  font-size: 0.74rem;
}

.move-dialog-row-chevron {
  flex: 0 0 auto;
  color: var(--vf-text-subtle);
}

.move-dialog-warning {
  display: flex;
  align-items: flex-start;
  gap: 0.35rem;
  margin: 0;
  padding: 0.45rem 0.55rem;
  border: 1px solid var(--vf-warning-line);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-warning-soft);
  color: var(--vf-text);
  font-size: 0.78rem;
  line-height: 1.5;
}

.move-dialog-warning-icon {
  flex: 0 0 auto;
  margin-top: 0.1rem;
  color: var(--vf-warning-text);
}
</style>
