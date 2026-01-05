import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { filesService } from '../services/files.service';
import type { FileInfo } from '../types';

export const useFilesStore = defineStore('files', () => {
  // 状态
  const files = ref<FileInfo[]>([]);
  const currentPath = ref<string>('');
  const loading = ref(false);
  const error = ref<string | null>(null);

  // 计算属性
  const breadcrumbs = computed(() => {
    if (!currentPath.value) return [{ name: '根目录', path: '' }];

    const parts = currentPath.value.split('/');
    const crumbs = [{ name: '根目录', path: '' }];

    let path = '';
    for (const part of parts) {
      path = path ? `${path}/${part}` : part;
      crumbs.push({ name: part, path });
    }

    return crumbs;
  });

  const directories = computed(() => {
    return files.value.filter((f) => f.type === 'directory');
  });

  const regularFiles = computed(() => {
    return files.value.filter((f) => f.type === 'file');
  });

  // 方法
  async function loadFiles(path: string = '') {
    loading.value = true;
    error.value = null;

    try {
      files.value = await filesService.getFiles(path);
      currentPath.value = path;
    } catch (err) {
      error.value = err instanceof Error ? err.message : '加载文件失败';
      files.value = [];
    } finally {
      loading.value = false;
    }
  }

  async function uploadFile(file: File, message?: string) {
    loading.value = true;
    error.value = null;

    try {
      await filesService.uploadFile(file, currentPath.value, message);
      await loadFiles(currentPath.value);
    } catch (err) {
      error.value = err instanceof Error ? err.message : '上传失败';
      throw err;
    } finally {
      loading.value = false;
    }
  }

  async function deleteFile(path: string, message?: string) {
    loading.value = true;
    error.value = null;

    try {
      await filesService.deleteFile(path, message);
      await loadFiles(currentPath.value);
    } catch (err) {
      error.value = err instanceof Error ? err.message : '删除失败';
      throw err;
    } finally {
      loading.value = false;
    }
  }

  function navigateTo(path: string) {
    loadFiles(path);
  }

  function goBack() {
    if (!currentPath.value) return;

    const parts = currentPath.value.split('/');
    parts.pop();
    navigateTo(parts.join('/'));
  }

  return {
    files,
    currentPath,
    loading,
    error,
    breadcrumbs,
    directories,
    regularFiles,
    loadFiles,
    uploadFile,
    deleteFile,
    navigateTo,
    goBack,
  };
});
