<template>
  <div v-if="desktop" class="table-container">
    <table class="table is-fullwidth is-hoverable is-narrow is-striped">
      <thead>
        <tr>
          <th class="is-narrow"></th>
          <th>名称</th>
          <th class="is-narrow">修改日期</th>
          <th class="is-narrow">类型</th>
          <th class="is-narrow has-text-right">大小</th>
          <th class="is-narrow has-text-right">操作</th>
        </tr>
      </thead>
      <tbody>
        <FileItem
          v-for="file in files"
          :key="file.path"
          :file="file"
          :highlight="highlight"
          :select-mode="selectMode"
          :selected="selectedPaths.has(file.path) || (!selectMode && activePath === file.path)"
          :expanded="expandedPath === file.path"
          :desktop="true"
          @click="emit('click', file)"
          @download="emit('download', file)"
          @rename="emit('rename', file)"
          @move="emit('move', file)"
          @delete="emit('delete', file)"
          @view-history="emit('view-history', file)"
          @toggle-select="emit('toggle-select', file)"
          @share="emit('share', file)"
          @preview="emit('preview', file)"
          @open-folder="emit('open-folder', file)"
          @create-directory="emit('create-directory', file)"
        />
      </tbody>
    </table>
  </div>

  <template v-else>
    <FileItem
      v-for="file in files"
      :key="file.path"
      :file="file"
      :highlight="highlight"
      :select-mode="selectMode"
      :selected="selectedPaths.has(file.path)"
      :expanded="expandedPath === file.path"
      @click="emit('click', file)"
      @download="emit('download', file)"
      @rename="emit('rename', file)"
      @move="emit('move', file)"
      @delete="emit('delete', file)"
      @view-history="emit('view-history', file)"
      @toggle-select="emit('toggle-select', file)"
      @share="emit('share', file)"
      @preview="emit('preview', file)"
      @open-folder="emit('open-folder', file)"
      @create-directory="emit('create-directory', file)"
    />
  </template>
</template>

<script setup lang="ts">
import { toRefs } from "vue";
import type { FileInfo } from "../../types";
import FileItem from "./FileItem.vue";

const emit = defineEmits<{
  (e: "click", file: FileInfo): void;
  (e: "download", file: FileInfo): void;
  (e: "rename", file: FileInfo): void;
  (e: "move", file: FileInfo): void;
  (e: "delete", file: FileInfo): void;
  (e: "view-history", file: FileInfo): void;
  (e: "toggle-select", file: FileInfo): void;
  (e: "share", file: FileInfo): void;
  (e: "preview", file: FileInfo): void;
  (e: "open-folder", file: FileInfo): void;
  (e: "create-directory", file: FileInfo): void;
}>();

const props = withDefaults(
  defineProps<{
    files: FileInfo[];
    highlight?: string;
    selectMode: boolean;
    selectedPaths: Set<string>;
    expandedPath?: string;
    activePath?: string;
    desktop?: boolean;
  }>(),
  {
    highlight: "",
    expandedPath: "",
    activePath: "",
    desktop: false,
  },
);

const { files, highlight, selectMode, selectedPaths, expandedPath, activePath } =
  toRefs(props);
</script>
