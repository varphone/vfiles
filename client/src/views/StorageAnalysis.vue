<template>
  <FilesLayout title="空间分析">
    <template #sidebar>
      <ViewsSidebar @search="goToSearch" />
    </template>

    <div class="storage-view">
      <PageHeader
        title="空间分析"
        description="按文件夹查看当前目录的存储占用，点击文件夹可继续下钻"
      >
        <template #actions>
          <button
            class="vf-ghost-button"
            type="button"
            :disabled="loading"
            @click="reload"
          >
            <IconRefresh
              :size="16"
              :class="{ 'is-spinning': loading }"
              aria-hidden="true"
            />
            <span>刷新</span>
          </button>
        </template>
      </PageHeader>

      <nav class="storage-breadcrumb" aria-label="空间分析路径">
        <button
          v-if="path"
          class="vf-ghost-button storage-back"
          type="button"
          aria-label="返回上一级"
          @click="navigateTo(parentPath)"
        >
          <IconArrowLeft :size="16" aria-hidden="true" />
          <span>上一级</span>
        </button>
        <ol class="storage-crumbs">
          <li v-for="(crumb, index) in breadcrumbs" :key="crumb.path">
            <IconChevronRight
              v-if="index > 0"
              :size="14"
              aria-hidden="true"
            />
            <button
              class="storage-crumb"
              :class="{ 'is-current': index === breadcrumbs.length - 1 }"
              type="button"
              :aria-current="
                index === breadcrumbs.length - 1 ? 'page' : undefined
              "
              @click="navigateTo(crumb.path)"
            >
              {{ crumb.label }}
            </button>
          </li>
        </ol>
      </nav>

      <EmptyState
        v-if="error"
        :icon="IconAlertCircle"
        tone="error"
        title="加载空间分析失败"
        :hint="error"
      >
        <template #actions>
          <button class="vf-ghost-button is-primary" type="button" @click="reload">
            <IconRefresh :size="16" />
            <span>重试</span>
          </button>
        </template>
      </EmptyState>

      <SkeletonList
        v-else-if="loading && !analysis"
        :rows="6"
        label="加载空间分析"
      />

      <template v-else-if="analysis">
        <EmptyState
          v-if="analysis.total_bytes === 0 && analysis.folders.length === 0"
          :icon="IconChartPie"
          title="当前目录还没有文件"
          hint="上传文件或创建目录后，这里会显示空间占用"
        />

        <template v-else>
          <section class="storage-hero" aria-label="存储占用概览">
            <div class="storage-donut-wrap">
              <div
                class="storage-donut"
                :style="{ background: donutBackground }"
                role="img"
                :aria-label="`当前目录共 ${formatSize(analysis.total_bytes)}，${donutSegments.map((segment) => `${segment.name} ${percent(segment.bytes)}%`).join('，')}`"
              >
                <div class="storage-donut-hole">
                  <span class="storage-donut-label">已用空间</span>
                  <strong class="storage-donut-value">
                    {{ formatSize(analysis.total_bytes) }}
                  </strong>
                  <span class="storage-donut-meta">
                    {{ analysis.file_count }} 个文件
                  </span>
                </div>
              </div>
            </div>

            <div class="storage-summary">
              <div class="storage-summary-head">
                <div>
                  <p class="storage-summary-kicker">
                    {{ path ? "当前文件夹" : "全部文件" }}
                  </p>
                  <h2 class="storage-summary-title">
                    {{ path || "根目录" }}
                  </h2>
                </div>
                <span class="storage-summary-total">
                  {{ formatSize(analysis.total_bytes) }}
                </span>
              </div>

              <ul class="storage-legend">
                <li
                  v-for="segment in donutSegments"
                  :key="segment.key"
                  class="storage-legend-item"
                >
                  <span
                    class="storage-legend-dot"
                    :style="{ background: segment.color }"
                    aria-hidden="true"
                  />
                  <span class="storage-legend-name" :title="segment.path">
                    {{ segment.name }}
                  </span>
                  <span class="storage-legend-size">
                    {{ formatSize(segment.bytes) }}
                  </span>
                  <span class="storage-legend-percent">
                    {{ percent(segment.bytes) }}%
                  </span>
                </li>
              </ul>

              <p class="storage-summary-foot">
                {{ analysis.direct_file_count }} 个直属文件
                · {{ analysis.directory_count }} 个目录
              </p>
            </div>
          </section>

          <div class="storage-columns">
            <section class="storage-panel" aria-label="文件夹占用">
              <header class="storage-panel-head">
                <div>
                  <h2 class="storage-panel-title">文件夹</h2>
                  <p class="storage-panel-hint">包含子目录中的全部文件</p>
                </div>
                <span class="storage-panel-count">
                  {{ analysis.folders.length }}
                </span>
              </header>

              <EmptyState
                v-if="analysis.folders.length === 0"
                :icon="IconFolder"
                compact
                title="没有子文件夹"
                hint="当前目录的文件都在右侧列表中"
              />
              <ul v-else class="storage-folder-list">
                <li
                  v-for="(folder, index) in analysis.folders"
                  :key="folder.path"
                >
                  <button
                    class="storage-folder"
                    type="button"
                    :aria-label="`进入 ${folder.name}，${formatSize(folder.bytes)}，占 ${percent(folder.bytes)}%`"
                    @click="navigateTo(folder.path)"
                  >
                    <span class="storage-folder-icon">
                      <IconFolder :size="18" aria-hidden="true" />
                    </span>
                    <span class="storage-folder-main">
                      <span class="storage-folder-name">{{ folder.name }}</span>
                      <span class="storage-folder-meta">
                        {{ folder.file_count }} 个文件 ·
                        {{ folder.directory_count }} 个目录
                      </span>
                      <span class="storage-folder-bar" aria-hidden="true">
                        <span
                          class="storage-folder-bar-fill"
                          :style="{
                            width: `${percent(folder.bytes)}%`,
                            background: colorFor(index),
                          }"
                        />
                      </span>
                    </span>
                    <span class="storage-folder-size">
                      {{ formatSize(folder.bytes) }}
                    </span>
                    <span class="storage-folder-percent">
                      {{ percent(folder.bytes) }}%
                    </span>
                    <IconChevronRight :size="16" aria-hidden="true" />
                  </button>
                </li>
              </ul>

              <div
                v-if="analysis.direct_file_bytes > 0"
                class="storage-direct"
                :aria-label="`直属文件共 ${formatSize(analysis.direct_file_bytes)}`"
              >
                <span class="storage-direct-icon">
                  <IconFiles :size="18" aria-hidden="true" />
                </span>
                <span class="storage-direct-main">
                  <span class="storage-direct-name">直属文件</span>
                  <span class="storage-direct-meta">
                    {{ analysis.direct_file_count }} 个文件
                  </span>
                </span>
                <span class="storage-direct-size">
                  {{ formatSize(analysis.direct_file_bytes) }}
                </span>
                <span class="storage-direct-percent">
                  {{ percent(analysis.direct_file_bytes) }}%
                </span>
              </div>
            </section>

            <section class="storage-panel" aria-label="文件占用">
              <header class="storage-panel-head">
                <div>
                  <h2 class="storage-panel-title">最大的文件</h2>
                  <p class="storage-panel-hint">当前目录子树中占用最多</p>
                </div>
              </header>

              <EmptyState
                v-if="analysis.largest_files.length === 0"
                :icon="IconFile"
                compact
                title="没有文件"
                hint="上传文件后，这里会显示最大文件"
              />
              <ol v-else class="storage-file-list">
                <li
                  v-for="file in analysis.largest_files"
                  :key="file.path"
                  class="storage-file"
                >
                  <span class="storage-file-icon">
                    <IconFile :size="17" aria-hidden="true" />
                  </span>
                  <span class="storage-file-main">
                    <span class="storage-file-name" :title="file.path">
                      {{ file.name }}
                    </span>
                    <span class="storage-file-path" :title="file.path">
                      {{ file.path }}
                    </span>
                  </span>
                  <span class="storage-file-size">
                    {{ formatSize(file.bytes) }}
                  </span>
                  <span class="storage-file-percent">
                    {{ percent(file.bytes) }}%
                  </span>
                </li>
              </ol>

              <p class="storage-panel-foot">
                <IconInfoCircle :size="14" aria-hidden="true" />
                <span>占比按当前目录的文件总大小计算</span>
              </p>
            </section>
          </div>
        </template>
      </template>
    </div>
  </FilesLayout>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  IconAlertCircle,
  IconArrowLeft,
  IconChartPie,
  IconChevronRight,
  IconFile,
  IconFiles,
  IconFolder,
  IconInfoCircle,
  IconRefresh,
} from "@tabler/icons-vue";
import FilesLayout from "../layouts/FilesLayout.vue";
import ViewsSidebar from "../components/files/ViewsSidebar.vue";
import PageHeader from "../ui/PageHeader.vue";
import EmptyState from "../components/common/EmptyState.vue";
import SkeletonList from "../components/common/SkeletonList.vue";
import { filesService } from "../services/files.service";
import { formatSize } from "../utils/filePresentation";
import { normalizeScopePath } from "../utils/sessionScope";
import type { SpaceAnalysis } from "../types";

const COLORS = [
  "var(--vf-chart-document)",
  "var(--vf-chart-image)",
  "var(--vf-chart-video)",
  "var(--vf-chart-audio)",
  "var(--vf-chart-other)",
];

const route = useRoute();
const router = useRouter();
const analysis = ref<SpaceAnalysis | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);
let requestId = 0;

const path = computed(() => {
  const raw = typeof route.query.path === "string" ? route.query.path : "";
  return normalizeScopePath(raw);
});

const breadcrumbs = computed(() => {
  const segments = path.value.split("/").filter(Boolean);
  const items = [{ label: "全部文件", path: "" }];
  let current = "";
  for (const segment of segments) {
    current = current ? `${current}/${segment}` : segment;
    items.push({ label: segment, path: current });
  }
  return items;
});

const parentPath = computed(() => {
  const index = path.value.lastIndexOf("/");
  return index < 0 ? "" : path.value.substring(0, index);
});

function percent(bytes: number): string {
  const total = analysis.value?.total_bytes ?? 0;
  if (total <= 0) return "0.0";
  return ((bytes / total) * 100).toFixed(1);
}

function colorFor(index: number): string {
  return COLORS[index % COLORS.length];
}

const donutSegments = computed(() => {
  if (!analysis.value) return [];
  const folders = analysis.value.folders;
  const visible = folders.slice(0, 5).map((folder, index) => ({
    key: `folder:${folder.path}`,
    path: folder.path,
    name: folder.name,
    bytes: folder.bytes,
    color: colorFor(index),
  }));
  const rest = folders.slice(5).reduce((sum, folder) => sum + folder.bytes, 0);
  if (rest > 0) {
    visible.push({
      key: "folders:other",
      path: "",
      name: "其他文件夹",
      bytes: rest,
      color: COLORS[COLORS.length - 1],
    });
  }
  if (analysis.value.direct_file_bytes > 0) {
    visible.push({
      key: "files:direct",
      path: "",
      name: "直属文件",
      bytes: analysis.value.direct_file_bytes,
      color: "var(--vf-chart-other)",
    });
  }
  return visible.filter((segment) => segment.bytes > 0);
});

const donutBackground = computed(() => {
  const total = analysis.value?.total_bytes ?? 0;
  if (total <= 0 || donutSegments.value.length === 0) {
    return "var(--vf-surface-2)";
  }
  let cursor = 0;
  const stops = donutSegments.value.map((segment) => {
    const start = (cursor / total) * 100;
    cursor += segment.bytes;
    const end = (cursor / total) * 100;
    return `${segment.color} ${start}% ${end}%`;
  });
  return `conic-gradient(${stops.join(", ")})`;
});

async function load() {
  const id = ++requestId;
  loading.value = true;
  error.value = null;
  try {
    const data = await filesService.getSpaceAnalysis(path.value);
    if (id !== requestId) return;
    analysis.value = data;
  } catch (err) {
    if (id !== requestId) return;
    analysis.value = null;
    error.value = err instanceof Error ? err.message : "加载空间分析失败";
  } finally {
    if (id === requestId) loading.value = false;
  }
}

function reload() {
  void load();
}

function navigateTo(nextPath: string) {
  const normalized = normalizeScopePath(nextPath);
  const query = normalized ? { path: normalized } : {};
  if (normalized === path.value) return;
  void router.push({ path: "/storage", query });
}

function goToSearch(query: string) {
  void router.push({ path: "/", query: { q: query } });
}

watch(path, () => void load(), { immediate: true });
</script>

<style scoped>
/* 页面在圆角内容面板内部滚动：面板本身 overflow: hidden。 */
.storage-view {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  gap: var(--vf-space-4);
  min-width: 0;
  min-height: 0;
  overflow-y: auto;
}

.storage-breadcrumb {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  min-height: 32px;
}

.storage-back {
  flex: 0 0 auto;
}

.storage-crumbs {
  display: flex;
  align-items: center;
  gap: var(--vf-space-1);
  margin: 0;
  padding: 0;
  min-width: 0;
  list-style: none;
  overflow: hidden;
}

.storage-crumbs li {
  display: flex;
  align-items: center;
  gap: var(--vf-space-1);
  min-width: 0;
  color: var(--vf-text-muted);
}

.storage-crumb {
  border: 0;
  background: transparent;
  padding: 4px 6px;
  border-radius: var(--vf-radius-control);
  color: var(--vf-accent);
  font: inherit;
  font-size: var(--vf-text-sm);
  white-space: nowrap;
  cursor: pointer;
}

.storage-crumb:hover {
  background: var(--vf-row-hover);
}

.storage-crumb.is-current {
  color: var(--vf-text);
  font-weight: 600;
  cursor: default;
}

.storage-hero {
  display: grid;
  grid-template-columns: minmax(190px, 250px) minmax(0, 1fr);
  gap: var(--vf-space-5);
  align-items: center;
  padding: var(--vf-space-5);
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-card);
  background: var(--vf-surface);
}

.storage-donut-wrap {
  display: flex;
  justify-content: center;
}

.storage-donut {
  position: relative;
  width: 210px;
  height: 210px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  box-shadow: inset 0 0 0 1px var(--vf-border-weak);
}

.storage-donut-hole {
  width: 132px;
  height: 132px;
  border-radius: 50%;
  background: var(--vf-surface);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 2px;
  text-align: center;
  box-shadow: 0 0 0 1px var(--vf-border-weak);
}

.storage-donut-label {
  color: var(--vf-text-muted);
  font-size: var(--vf-text-xs);
}

.storage-donut-value {
  color: var(--vf-text);
  font-size: var(--vf-text-lg);
  font-weight: 700;
}

.storage-donut-meta {
  color: var(--vf-text-muted);
  font-size: var(--vf-text-xs);
}

.storage-summary {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--vf-space-3);
}

.storage-summary-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--vf-space-3);
}

.storage-summary-kicker {
  margin: 0 0 2px;
  color: var(--vf-text-muted);
  font-size: var(--vf-text-xs);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.storage-summary-title {
  margin: 0;
  color: var(--vf-text);
  font-size: var(--vf-text-xl);
  font-weight: 700;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.storage-summary-total {
  flex: 0 0 auto;
  color: var(--vf-text);
  font-size: var(--vf-text-lg);
  font-weight: 700;
}

.storage-legend {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
  gap: var(--vf-space-2) var(--vf-space-4);
  margin: 0;
  padding: 0;
  list-style: none;
}

.storage-legend-item {
  display: grid;
  grid-template-columns: 10px minmax(0, 1fr) auto auto;
  align-items: center;
  gap: var(--vf-space-2);
  min-width: 0;
  font-size: var(--vf-text-sm);
}

.storage-legend-dot {
  width: 10px;
  height: 10px;
  border-radius: 3px;
}

.storage-legend-name {
  color: var(--vf-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.storage-legend-size,
.storage-legend-percent {
  color: var(--vf-text-muted);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.storage-legend-percent {
  min-width: 42px;
  text-align: right;
}

.storage-summary-foot {
  margin: 0;
  color: var(--vf-text-muted);
  font-size: var(--vf-text-sm);
}

.storage-columns {
  display: grid;
  grid-template-columns: minmax(0, 1.35fr) minmax(300px, 1fr);
  gap: var(--vf-space-4);
  align-items: start;
}

.storage-panel {
  min-width: 0;
  padding: var(--vf-space-4);
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-card);
  background: var(--vf-surface);
}

.storage-panel-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--vf-space-3);
  padding-bottom: var(--vf-space-3);
  border-bottom: 1px solid var(--vf-border-weak);
}

.storage-panel-title {
  margin: 0;
  color: var(--vf-text);
  font-size: var(--vf-text-base);
  font-weight: 700;
}

.storage-panel-hint {
  margin: 2px 0 0;
  color: var(--vf-text-muted);
  font-size: var(--vf-text-xs);
}

.storage-panel-count {
  min-width: 26px;
  padding: 2px 8px;
  border-radius: 999px;
  background: var(--vf-surface-2);
  color: var(--vf-text-muted);
  font-size: var(--vf-text-xs);
  text-align: center;
}

.storage-folder-list,
.storage-file-list {
  margin: 0;
  padding: 0;
  list-style: none;
}

.storage-folder {
  width: 100%;
  display: grid;
  grid-template-columns: 30px minmax(0, 1fr) auto auto 18px;
  align-items: center;
  gap: var(--vf-space-2);
  padding: 12px 4px;
  border: 0;
  border-bottom: 1px solid var(--vf-border-weak);
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.storage-folder-list li:last-child .storage-folder {
  border-bottom: 0;
}

.storage-folder:hover {
  background: var(--vf-row-hover);
}

.storage-folder-icon,
.storage-direct-icon,
.storage-file-icon {
  display: grid;
  place-items: center;
  color: var(--vf-accent);
}

.storage-folder-main,
.storage-direct-main,
.storage-file-main {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.storage-folder-name,
.storage-direct-name,
.storage-file-name {
  color: var(--vf-text);
  font-size: var(--vf-text-sm);
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.storage-folder-meta,
.storage-direct-meta,
.storage-file-path {
  color: var(--vf-text-muted);
  font-size: var(--vf-text-xs);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.storage-folder-bar {
  display: block;
  width: 100%;
  height: 4px;
  margin-top: 2px;
  border-radius: 999px;
  background: var(--vf-surface-2);
  overflow: hidden;
}

.storage-folder-bar-fill {
  display: block;
  height: 100%;
  border-radius: inherit;
}

.storage-folder-size,
.storage-direct-size,
.storage-file-size {
  color: var(--vf-text);
  font-size: var(--vf-text-sm);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.storage-folder-percent,
.storage-direct-percent,
.storage-file-percent {
  color: var(--vf-text-muted);
  font-size: var(--vf-text-xs);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  min-width: 44px;
  text-align: right;
}

.storage-direct {
  display: grid;
  grid-template-columns: 30px minmax(0, 1fr) auto auto;
  align-items: center;
  gap: var(--vf-space-2);
  padding: 12px 4px;
  border-top: 1px solid var(--vf-border-weak);
}

.storage-direct-icon {
  color: var(--vf-text-muted);
}

.storage-file {
  display: grid;
  grid-template-columns: 24px minmax(0, 1fr) auto auto;
  align-items: center;
  gap: var(--vf-space-2);
  padding: 10px 4px;
  border-bottom: 1px solid var(--vf-border-weak);
}

.storage-file-list li:last-child .storage-file {
  border-bottom: 0;
}

.storage-file-icon {
  color: var(--vf-text-muted);
}

.storage-panel-foot {
  display: flex;
  align-items: center;
  gap: var(--vf-space-1);
  margin: var(--vf-space-3) 0 0;
  padding-top: var(--vf-space-3);
  border-top: 1px solid var(--vf-border-weak);
  color: var(--vf-text-muted);
  font-size: var(--vf-text-xs);
}

@media screen and (max-width: 1023px) {
  .storage-hero {
    grid-template-columns: 1fr;
    justify-items: center;
  }

  .storage-summary {
    width: 100%;
  }

  .storage-columns {
    grid-template-columns: 1fr;
  }
}

@media screen and (max-width: 600px) {
  .storage-hero {
    padding: var(--vf-space-4);
  }

  .storage-donut {
    width: 180px;
    height: 180px;
  }

  .storage-donut-hole {
    width: 112px;
    height: 112px;
  }

  .storage-folder {
    grid-template-columns: 28px minmax(0, 1fr) auto 16px;
  }

  .storage-folder-percent {
    display: none;
  }

  .storage-legend {
    grid-template-columns: 1fr;
  }

  .storage-summary-title {
    max-width: 60vw;
  }
}
</style>
