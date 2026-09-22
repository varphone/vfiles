<template>
  <div class="breadcrumb-tree" role="menu" aria-label="目录树">
    <div
      v-if="rootRows.length === 0 && !loadingRoot"
      class="breadcrumb-tree-empty"
    >
      当前目录没有子文件夹
    </div>

    <div
      v-for="row in rows"
      :key="row.path"
      class="breadcrumb-tree-row"
      :class="{ 'is-active': row.path === currentPath }"
      :style="{ paddingLeft: `${0.35 + row.depth * 0.85}rem` }"
    >
      <button
        class="breadcrumb-tree-twisty"
        type="button"
        :disabled="row.loading"
        :aria-label="`${row.expanded ? '收起' : '展开'} ${row.name}`"
        :aria-expanded="row.expanded ? 'true' : 'false'"
        @click.stop="toggle(row.path)"
      >
        <IconChevronRight
          :size="14"
          class="breadcrumb-tree-chevron"
          :class="{ 'is-expanded': row.expanded }"
        />
      </button>
      <button
        class="breadcrumb-tree-item"
        type="button"
        role="menuitem"
        :title="row.path"
        @click="emit('navigate', row.path)"
      >
        <IconFolder :size="14" class="breadcrumb-tree-icon" />
        <span class="breadcrumb-tree-name">{{ row.name }}</span>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { IconChevronRight, IconFolder } from "@tabler/icons-vue";
import { filesService } from "../../services/files.service";
import type { FileInfo } from "../../types";

/** 子目录一次最多取这么多同级条目。 */
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

/**
 * 面包屑下拉里的目录树。
 *
 * 与侧栏目录树一样按需加载：展开某层时才请求该目录的子目录并缓存，
 * 因此从面包屑就能沿层级快速跳转，而不用先进入目录。
 */
const props = withDefaults(
  defineProps<{
    /** 当前目录路径（用于高亮）。 */
    currentPath?: string;
    /** 当前目录的直接子目录（父组件已经拿到，避免重复请求）。 */
    directories?: FileInfo[];
  }>(),
  { currentPath: "", directories: () => [] },
);

const emit = defineEmits<{
  (e: "navigate", path: string): void;
}>();

const children = ref<Record<string, TreeChild[]>>({});
const expanded = ref<Set<string>>(new Set());
const loading = ref<Set<string>>(new Set());
const loadingRoot = ref(true);

const rootRows = computed(() => children.value[props.currentPath] ?? []);

async function loadChildren(path: string) {
  if (children.value[path] || loading.value.has(path)) return;
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
    // 失败按「没有子目录」处理，不影响菜单其它层级
    children.value = { ...children.value, [path]: [] };
  } finally {
    const next = new Set(loading.value);
    next.delete(path);
    loading.value = next;
    if (path === props.currentPath) loadingRoot.value = false;
  }
}

function toggle(path: string) {
  const next = new Set(expanded.value);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
  }
  expanded.value = next;
  void loadChildren(path);
}

/** 把「已展开 + 已加载」的层级拍平成带缩进的行。 */
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
  walk(props.currentPath, 0);
  return result;
});

onMounted(() => {
  // 第一层直接用父组件已有的数据，展开更深层时才发请求
  children.value = {
    ...children.value,
    [props.currentPath]: props.directories.map((item) => ({
      path: item.path,
      name: item.name,
    })),
  };
  loadingRoot.value = false;
});
</script>

<style scoped>
.breadcrumb-tree {
  min-width: 200px;
  max-height: 320px;
  overflow-y: auto;
  padding: 4px;
}

.breadcrumb-tree-empty {
  padding: 0.45rem 0.6rem;
  font-size: 0.8rem;
  color: var(--vf-text-subtle);
}

.breadcrumb-tree-row {
  display: flex;
  align-items: center;
  gap: 0.1rem;
  border-radius: var(--vf-radius-sm);
}

.breadcrumb-tree-row.is-active .breadcrumb-tree-item {
  color: var(--vf-accent-text);
  font-weight: 600;
}

.breadcrumb-tree-twisty {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: 0 0 auto;
  width: 1.15rem;
  height: 1.6rem;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--vf-text-subtle);
  cursor: pointer;
}

.breadcrumb-tree-chevron {
  transition: transform 0.15s var(--vf-motion-standard);
}

.breadcrumb-tree-chevron.is-expanded {
  transform: rotate(90deg);
}

.breadcrumb-tree-item {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  flex: 1 1 auto;
  min-width: 0;
  height: 1.7rem;
  padding: 0 0.4rem 0 0;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: var(--vf-text);
  font-size: 0.82rem;
  text-align: left;
  cursor: pointer;
}

.breadcrumb-tree-item:hover {
  background: var(--vf-surface-hover);
  color: var(--vf-text-strong);
}

.breadcrumb-tree-icon {
  flex: 0 0 auto;
  color: var(--vf-text-muted);
}

.breadcrumb-tree-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
