import { computed } from "vue";
import { useAuthStore } from "../stores/auth.store";
import { useFilesStore } from "../stores/files.store";
import { sliceBreadcrumbsFromScopeRoot } from "../utils/sessionScope";

/**
 * 受限会话的范围、写/分享能力与面包屑裁剪。
 *
 * 主密码会话恒为全量；受限密码会话按 scopeRoots 隐藏范围根之上的面包屑，
 * 并根据 canWrite 决定显示「可写/只读」。
 */
export function useFileBrowserScope() {
  const authStore = useAuthStore();
  const filesStore = useFilesStore();

  const isRestricted = computed(() => authStore.isRestricted);
  const scopeRoots = computed(() => authStore.scopeRoots);
  const canWrite = computed(() => authStore.canWrite);
  const canShare = computed(
    () =>
      authStore.features?.shareEnabled !== false &&
      (!isRestricted.value || canWrite.value),
  );

  /** 面包屑隐藏范围根之上的入口。 */
  const visibleBreadcrumbs = computed(() =>
    isRestricted.value
      ? sliceBreadcrumbsFromScopeRoot(
          filesStore.breadcrumbs,
          scopeRoots.value,
        )
      : filesStore.breadcrumbs,
  );

  const scopeBadgeText = computed(() => {
    const roots = scopeRoots.value.length
      ? scopeRoots.value.join(", ")
      : "受限目录";
    return `受限访问：${roots}（${canWrite.value ? "可写" : "只读"}）`;
  });

  const scopeBadgeTitle = computed(() => {
    const label = authStore.scope?.label;
    return label ? `受限密码：${label}` : scopeBadgeText.value;
  });

  return {
    isRestricted,
    scopeRoots,
    canWrite,
    canShare,
    visibleBreadcrumbs,
    scopeBadgeText,
    scopeBadgeTitle,
  };
}
