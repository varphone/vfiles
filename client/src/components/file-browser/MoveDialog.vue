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
              v-if="!hasScopeRoots"
              class="move-dialog-crumb"
              :class="{ 'is-current': !browserPath }"
              type="button"
              @click="goRoot"
            >
              <IconHome :size="14" />
              <span>根目录</span>
            </button>
            <template v-for="crumb in visibleBreadcrumbs" :key="crumb.path">
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

          <div
            v-if="directoryTotal > DIRECTORY_PAGE_SIZE"
            class="move-dialog-pagination"
            aria-label="子目录分页"
          >
            <button
              class="vf-ghost-button"
              type="button"
              :disabled="loading || directoryOffset === 0"
              @click="
                loadDirectories(
                  browserPath,
                  directoryOffset - DIRECTORY_PAGE_SIZE,
                )
              "
            >
              上一页
            </button>
            <span>
              第 {{ Math.floor(directoryOffset / DIRECTORY_PAGE_SIZE) + 1 }} /
              {{ Math.ceil(directoryTotal / DIRECTORY_PAGE_SIZE) }} 页，共
              {{ directoryTotal }} 个子文件夹
            </span>
            <button
              class="vf-ghost-button"
              type="button"
              :disabled="loading || !directoryHasMore"
              @click="
                loadDirectories(
                  browserPath,
                  directoryOffset + DIRECTORY_PAGE_SIZE,
                )
              "
            >
              下一页
            </button>
          </div>
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
import {
  filesService,
  MAX_MOVE_BATCH_ITEMS,
} from "../../services/files.service";
import type { FileInfo } from "../../types";
import {
  firstScopeRoot,
  isPathWithinRoots,
  normalizeScopeRoots,
  scopedParentPath,
  sliceBreadcrumbsFromScopeRoot,
} from "../../utils/sessionScope";

type MoveDialogItem = Pick<FileInfo, "name" | "path" | "kind">;
const DIRECTORY_PAGE_SIZE = 100;

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
    /** 受限会话允许访问的范围根；非空时目标目录不能跳到范围之外。 */
    roots?: string[];
  }>(),
  {
    initialPath: "",
    confirmLoading: false,
    roots: () => [],
  },
);

const scopedRoots = computed(() => normalizeScopeRoots(props.roots));
const hasScopeRoots = computed(() => scopedRoots.value.length > 0);

const emit = defineEmits<{
  close: [];
  confirm: [targetDir: string];
}>();

const browserPath = ref("");
const browserEntries = ref<FileInfo[]>([]);
const directoryOffset = ref(0);
const directoryTotal = ref(0);
const directoryHasMore = ref(false);
const loading = ref(false);
const error = ref("");
let directoryRequestId = 0;

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
  visibleBreadcrumbs.value.length
    ? `「${visibleBreadcrumbs.value[visibleBreadcrumbs.value.length - 1]?.name}」`
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

const visibleBreadcrumbs = computed(() =>
  hasScopeRoots.value
    ? sliceBreadcrumbsFromScopeRoot(breadcrumbs.value, scopedRoots.value)
    : breadcrumbs.value,
);

const validationMessages = computed(() => {
  if (loading.value) return [] as string[];

  const issues = new Set<string>();
  if (props.items.length > MAX_MOVE_BATCH_ITEMS) {
    issues.add(`一次最多移动 ${MAX_MOVE_BATCH_ITEMS} 个项目`);
    return Array.from(issues);
  }
  const usedTargets = new Set<string>();

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
    const requested = normalizePath(initialPath ?? "");
    const initial =
      hasScopeRoots.value && !isPathWithinRoots(requested, scopedRoots.value)
        ? firstScopeRoot(scopedRoots.value)
        : requested;
    browserPath.value = initial;
    await loadDirectories(initial);
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

async function loadDirectories(path: string, offset = 0) {
  const requestId = ++directoryRequestId;
  const normalizedPath = normalizePath(path);
  if (
    hasScopeRoots.value &&
    !isPathWithinRoots(normalizedPath, scopedRoots.value)
  ) {
    return;
  }
  loading.value = true;
  error.value = "";

  try {
    const page = await filesService.getDirectoriesPage(normalizedPath, {
      limit: DIRECTORY_PAGE_SIZE,
      offset: Math.max(0, offset),
    });
    if (requestId !== directoryRequestId) return;
    browserPath.value = normalizedPath;
    browserEntries.value = page.items;
    directoryOffset.value = page.offset;
    directoryTotal.value = page.total;
    directoryHasMore.value = page.has_more;
  } catch (err) {
    if (requestId !== directoryRequestId) return;
    browserEntries.value = [];
    directoryOffset.value = 0;
    directoryTotal.value = 0;
    directoryHasMore.value = false;
    error.value = err instanceof Error ? err.message : "目录加载失败";
  } finally {
    if (requestId === directoryRequestId) loading.value = false;
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
  if (hasScopeRoots.value && !isPathWithinRoots(nextPath, scopedRoots.value)) {
    return;
  }
  browserPath.value = nextPath;
  await loadDirectories(nextPath);
}

async function goRoot() {
  await openDirectory(
    hasScopeRoots.value ? firstScopeRoot(scopedRoots.value) : "",
  );
}

async function goUp() {
  if (!browserPath.value) return;
  if (hasScopeRoots.value) {
    const parent = scopedParentPath(browserPath.value, scopedRoots.value);
    if (parent == null) return;
    await openDirectory(parent);
    return;
  }
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
  background: var(--vf-surface-2);
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
  font-size: var(--vf-text-base);
  font-weight: 600;
  color: var(--vf-text);
}

.move-dialog-items-more {
  flex: 0 0 auto;
  color: var(--vf-text-subtle);
  font-size: var(--vf-text-sm);
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
  font-size: var(--vf-text-base);
  cursor: pointer;
}

.move-dialog-crumb:hover {
  background: var(--vf-row-hover);
  color: var(--vf-text);
}

.move-dialog-crumb.is-current {
  color: var(--vf-text);
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

.move-dialog-pagination {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  padding-top: 0.25rem;
  color: var(--vf-text-subtle);
  font-size: var(--vf-text-sm);
}

.move-dialog-pagination .vf-ghost-button {
  flex: 0 0 auto;
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
  font-size: var(--vf-text-base);
  text-align: left;
  cursor: pointer;
}

.move-dialog-row:hover:not(:disabled) {
  background: var(--vf-row-hover);
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
  font-size: var(--vf-text-xs);
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
  font-size: var(--vf-text-sm);
  line-height: 1.5;
}

.move-dialog-warning-icon {
  flex: 0 0 auto;
  margin-top: 0.1rem;
  color: var(--vf-warning-text);
}
</style>
