<template>
  <nav class="directory-tree" aria-label="目录树">
    <div class="directory-tree-scroll">
      <button
        class="directory-tree-row is-root"
        :class="{ 'is-active': !currentPath }"
        type="button"
        :aria-current="!currentPath ? 'true' : undefined"
        @click="emit('navigate', '')"
      >
        <IconFolder :size="16" class="directory-tree-icon" />
        <span class="directory-tree-name">全部文件</span>
      </button>

      <div v-if="loadingRoot" class="directory-tree-hint">加载中...</div>

      <template v-else>
        <div
          v-for="row in rows"
          :key="row.path"
          class="directory-tree-item"
          :class="{ 'is-drag-over': dragOverPath === row.path }"
          @dragover.prevent="onDragOver(row.path)"
          @dragleave="onDragLeave(row.path)"
          @drop.prevent="onDrop(row.path)"
        >
          <button
            class="directory-tree-twisty"
            type="button"
            :disabled="row.loading"
            :title="row.expanded ? '收起' : '展开'"
            :aria-label="`${row.expanded ? '收起' : '展开'} ${row.name}`"
            :aria-expanded="row.expanded ? 'true' : 'false'"
            @click.stop="toggle(row.path)"
          >
            <IconChevronRight
              :size="14"
              class="directory-tree-chevron"
              :class="{ 'is-expanded': row.expanded }"
            />
          </button>
          <button
            class="directory-tree-row"
            :class="{ 'is-active': row.path === currentPath }"
            :style="{ paddingLeft: `${0.5 + row.depth * 0.85}rem` }"
            type="button"
            :title="row.path"
            :aria-current="row.path === currentPath ? 'true' : undefined"
            @click="emit('navigate', row.path)"
          >
            <IconFolder :size="15" class="directory-tree-icon" />
            <span class="directory-tree-name">{{ row.name }}</span>
          </button>

          <!-- 拖放目标提示：与列表行/网格卡片一致（round 13 语言） -->
          <span
            v-if="dragOverPath === row.path"
            class="desktop-drop-hint"
            aria-hidden="true"
          >
            移动到「{{ row.name }}」
          </span>
        </div>

        <p v-if="rows.length === 0" class="directory-tree-hint">
          当前工作区没有子目录
        </p>
      </template>
    </div>
  </nav>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { IconChevronRight, IconFolder } from "@tabler/icons-vue";
import { filesService } from "../../services/files.service";

/** 目录树里只展示目录，一次最多取这么多同级条目。 */
const TREE_PAGE_SIZE = 200;

interface TreeChild {
  path: string;
  name: string;
}

interface TreeRow extends TreeChild {
  depth: number;
  expanded: boolean;
  loading: boolean;
}

const props = withDefaults(
  defineProps<{
    /** 当前浏览目录（空字符串表示根目录）。 */
    currentPath?: string;
    /** 是否有条目正在被拖动（用于高亮可放置的目标）。 */
    dragging?: boolean;
    /** 每次数据变更（新建/删除/重命名/移动）后递增，用于刷新已加载的层级。 */
    refreshKey?: number;
  }>(),
  { currentPath: "", dragging: false, refreshKey: 0 },
);

const emit = defineEmits<{
  (e: "navigate", path: string): void;
  (e: "drop-on-folder", path: string): void;
}>();

/** 已加载的子目录（undefined 表示还没取过）。 */
const children = ref<Record<string, TreeChild[]>>({});
const expanded = ref<Set<string>>(new Set());
const loading = ref<Set<string>>(new Set());
const loadingRoot = ref(true);
const dragOverPath = ref("");

async function loadChildren(path: string, options: { force?: boolean } = {}) {
  if (loading.value.has(path)) return;
  if (children.value[path] && !options.force) return;
  loading.value = new Set(loading.value).add(path);

  try {
    const page = await filesService.getFilesPage(path, {
      limit: TREE_PAGE_SIZE,
      offset: 0,
    });
    const directories = page.items
      .filter((item) => item.kind === "directory")
      .map((item) => ({ path: item.path, name: item.name }))
      .sort((a, b) =>
        a.name.localeCompare(b.name, "zh-Hans-CN", { numeric: true }),
      );
    children.value = { ...children.value, [path]: directories };
  } catch {
    // 加载失败时按「没有子目录」处理，避免树里出现错误态污染整页
    children.value = { ...children.value, [path]: [] };
  } finally {
    const next = new Set(loading.value);
    next.delete(path);
    loading.value = next;
    if (path === "") loadingRoot.value = false;
  }
}

function toggle(path: string) {
  const next = new Set(expanded.value);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
    void loadChildren(path);
  }
  expanded.value = next;
}

/** 把「已展开 + 已加载」的目录拍平成带缩进的行，避免递归组件。 */
const rows = computed<TreeRow[]>(() => {
  const result: TreeRow[] = [];
  const walk = (parent: string, depth: number) => {
    for (const child of children.value[parent] ?? []) {
      const isExpanded = expanded.value.has(child.path);
      result.push({
        ...child,
        depth,
        expanded: isExpanded,
        loading: loading.value.has(child.path),
      });
      if (isExpanded) walk(child.path, depth + 1);
    }
  };
  walk("", 0);
  return result;
});

/** 当前目录的祖先链（用于自动展开与高亮）。 */
function ancestorPaths(path: string): string[] {
  if (!path) return [];
  const parts = path.split("/").filter(Boolean);
  return parts
    .map((_, index) => parts.slice(0, index + 1).join("/"))
    .slice(0, -1);
}

/** 展开到当前目录：逐级加载祖先，保证深层目录在树里可见。 */
async function revealCurrentPath(path: string) {
  const ancestors = ancestorPaths(path);
  if (ancestors.length === 0) return;

  let changed = false;
  const next = new Set(expanded.value);
  for (const ancestor of ancestors) {
    if (!next.has(ancestor)) {
      next.add(ancestor);
      changed = true;
    }
  }
  if (changed) expanded.value = next;

  for (const ancestor of ["", ...ancestors]) {
    await loadChildren(ancestor);
  }
}

onMounted(() => {
  void loadChildren("").then(() => revealCurrentPath(props.currentPath));
});

/**
 * 刷新已加载的层级。
 *
 * 目录树对每层做了缓存，新建/删除/重命名目录后必须显式重取，否则要等用户
 * 收起再展开才会更新。这里只重取「已经加载过」的层级（含根层），
 * 保留展开状态，不会把用户展开的树折叠回去。
 */
async function refreshLoadedLevels() {
  await loadChildren("", { force: true });

  // 已展开的层级（含当前目录的祖先）重新拉取
  const paths = new Set<string>([
    ...expanded.value,
    ...ancestorPaths(props.currentPath),
  ]);
  await Promise.all(
    [...paths].map((path) => loadChildren(path, { force: true })),
  );

  await revealCurrentPath(props.currentPath);
}

watch(
  () => props.refreshKey,
  () => {
    void refreshLoadedLevels();
  },
);

watch(
  () => props.currentPath,
  (path) => {
    void revealCurrentPath(path);
  },
);

function onDragOver(path: string) {
  if (!props.dragging) return;
  dragOverPath.value = path;
}

function onDragLeave(path: string) {
  if (dragOverPath.value === path) dragOverPath.value = "";
}

function onDrop(path: string) {
  dragOverPath.value = "";
  if (!props.dragging) return;
  emit("drop-on-folder", path);
}
</script>

<style scoped>
.directory-tree {
  display: flex;
  flex-direction: column;
  min-width: 0;
  /* 必须可压缩并占据剩余高度：
     flex 子项的自动最小高度是「内容最小尺寸」，目录多时整棵树会按内容撑高、
     顶出侧栏盖住状态栏，内部滚动条也不会出现（scrollHeight == clientHeight）。
     覆盖该自动最小值后 .directory-tree-scroll 才能真正裁剪并滚动；
     同时给 10rem 地板，矮窗口下不被下方概览挤没。 */
  flex: 1 1 auto;
  min-height: 10rem;
  overflow: hidden;
  border-right: 1px solid var(--vf-border-weak);
  background: var(--vf-surface);
}

.directory-tree-scroll {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  padding: 0.5rem 0.35rem;
}

.directory-tree-item {
  position: relative;
  display: flex;
  align-items: center;
  border-radius: var(--vf-radius-sm);
  /* 落点高亮（is-drag-over）渐入 */
  transition: background-color 0.15s var(--vf-motion-standard);
}

.directory-tree-item.is-drag-over {
  background: var(--vf-accent-soft);
  outline: 1px dashed var(--vf-accent);
}

/* 树条目没有行尾「⋯」按钮，chip 收紧右距 */
.directory-tree-item .desktop-drop-hint {
  right: 0.35rem;
}

.directory-tree-twisty {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: 0 0 auto;
  width: 1.25rem;
  height: 1.6rem;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--vf-text-subtle);
  cursor: pointer;
}

.directory-tree-chevron {
  transition: transform 0.15s var(--vf-motion-standard);
}

.directory-tree-chevron.is-expanded {
  transform: rotate(90deg);
}

.directory-tree-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  flex: 1 1 auto;
  min-width: 0;
  height: 1.9rem;
  padding: 0 0.5rem;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: var(--vf-text);
  font-size: 0.82rem;
  text-align: left;
  cursor: pointer;
}

.directory-tree-row.is-root {
  margin-bottom: 0.15rem;
  font-weight: 600;
}

.directory-tree-row:hover {
  background: var(--vf-surface-hover);
  color: var(--vf-text-strong);
}

.directory-tree-row.is-active {
  background: var(--vf-accent-soft);
  color: var(--vf-accent-text);
  font-weight: 600;
}

.directory-tree-icon {
  flex: 0 0 auto;
  color: var(--vf-text-muted);
}

.directory-tree-row.is-active .directory-tree-icon {
  color: var(--vf-accent);
}

.directory-tree-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.directory-tree-hint {
  padding: 0.5rem 0.6rem;
  font-size: 0.76rem;
  color: var(--vf-text-subtle);
}
</style>
