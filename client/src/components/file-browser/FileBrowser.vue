<template>
  <!-- 光标跟随拖拽 chip（VS Code/macOS 式）：拖动时显示条目名/多选数 -->
  <Teleport to="body">
    <!-- 定位前缀中央浮动 HUD（Finder 式醒目回执 r78 ✓ 与状态栏语境并存分工）。
         Transition 包裹 = 超时淡出质感（r79 ✓ 否则 v-if 瞬逝粗糙 ✗） -->
    <Transition name="vf-hud">
      <div
        v-if="typeAheadPrefix"
        class="desktop-typeahead-hud"
        aria-hidden="true"
      >
        {{ typeAheadPrefix }}
      </div>
    </Transition>
    <div
      v-if="draggingFile"
      class="desktop-drag-chip"
      :class="{
        'is-over-target': dragChipTarget,
        'is-invalid': dragChipInvalid,
      }"
      :style="{
        left: `${dragChipPos.x}px`,
        top: `${dragChipPos.y}px`,
      }"
      aria-hidden="true"
    >
      {{ dragChipLabel }}
    </div>
  </Teleport>
  <div
    class="file-browser"
    @touchstart="onTouchStart"
    @touchmove="onTouchMove"
    @touchend="onTouchEnd"
  >
    <div class="box file-browser-box">
      <!--
        Nextcloud 式单行文件工具栏（50px）：左侧＝汉堡 + 「+ 新建」+ 视图名/面包屑，
        右侧＝搜索、列表/网格切换、排序与详细信息开关。
        上传入口统一收进「+ 新建」菜单（移动端底栏的上传入口保留）。
      -->
      <div class="breadcrumb-bar file-toolbar">
        <div class="file-toolbar-leading">
          <FileBrowserToolbarLeading
            :show-sidebar-toggle="Boolean(sidebarControl) && !isMobile"
            :sidebar-visible="sidebarVisible"
            :show-write="!isMobile && canWrite"
            :upload="uploadIndicator"
            :is-restricted="isRestricted"
            :scope-text="scopeBadgeText"
            :scope-title="scopeBadgeTitle"
            @toggle-sidebar="toggleSidebar"
            @upload-files="openUploader('files')"
            @upload-folder="openUploader('directory')"
            @create-folder="requestCreateDirectoryFromToolbar"
            @open-uploader="openUploader('files')"
          />
          <FileBrowserToolbarHeading
            v-model:open="viewMenuOpen"
            :is-mobile="isMobile"
            :current-path="currentPath"
            :breadcrumbs="visibleBreadcrumbs"
            :directories="filesStore.directories"
            :show-shares="
              !isRestricted && authStore.features?.shareEnabled !== false
            "
            @navigate="handleBreadcrumbNavigate"
            @drop="handleDropOnFolder"
          />
        </div>

        <div class="file-toolbar-actions">
          <!-- 版本切换：与工具栏同一行靠右，移动端由页面底栏承担。 -->
          <FileBrowserHistoryControls
            v-if="!isMobile && historyStateLabel"
            :state-label="historyStateLabel"
            :loading="historyLoading"
            :error="historyError"
            :title="historyTitle"
            :has-commit="historyHasCommit"
            @fast-back="emit('history-fast-back')"
            @step-back="emit('history-step-back')"
            @step-forward="emit('history-step-forward')"
            @fast-forward="emit('history-fast-forward')"
          />

          <BrowserSearchBox
            v-if="!isMobile"
            :model-value="searchQuery"
            :open="desktopSearchOpen"
            :content="searchContent"
            :type="searchType"
            :scope-current="searchScopeCurrent"
            :loading="searchLoading"
            :active="searchActive"
            :content-enabled="searchContentEnabled"
            :filters-active="desktopSearchFiltersActive"
            :history="searchHistory"
            :register-input="setDesktopSearchInput"
            @update:model-value="searchQuery = $event"
            @update:open="desktopSearchOpen = $event"
            @update:content="searchContent = $event"
            @update:type="searchType = $event as SearchType"
            @update:scope-current="searchScopeCurrent = $event"
            @search="runDesktopSearch"
            @clear="clearDesktopSearch"
          />

          <FileBrowserViewControls v-if="!isMobile" />
        </div>
      </div>

      <!-- 移动端搜索行保持原样：底栏的上传/新建入口也随之不变。 -->
      <div v-if="isMobile" class="file-browser-toolbar">
        <MobileSearchBar
          v-model:query="searchQuery"
          v-model:content="searchContent"
          v-model:type="searchType"
          v-model:scope-current="searchScopeCurrent"
          v-model:filters-open="mobileSearchFiltersOpen"
          :loading="searchLoading"
          :active="searchActive"
          :content-enabled="searchContentEnabled"
          :history="searchHistory"
          :pull-indicator-visible="pullIndicatorVisible"
          :pull-refreshing="pullRefreshing"
          :pull-ready="pullReady"
          @search="runSearch"
          @clear="clearSearch"
          @enter-results="enterSearchResults"
        />

        <div
          v-if="searchError"
          class="notification is-danger is-light"
          role="alert"
        >
          <IconAlertCircle :size="20" class="mr-2" />
          {{ searchError }}
        </div>
      </div>

      <div
        v-else-if="searchError"
        class="file-toolbar-error notification is-danger is-light"
        role="alert"
      >
        <IconAlertCircle :size="20" class="mr-2" />
        {{ searchError }}
      </div>

      <div
        v-if="searchActive && searchResultsMayBeIncomplete"
        class="notification is-warning is-light mb-3"
        role="status"
      >
        搜索已达到资源上限，结果可能不完整。可缩小目录范围或调整搜索词后重试。
      </div>

      <DownloadQueuePanel
        :items="downloadQueue"
        :collapsed="queueCollapsed"
        :downloading="downloading"
        :active-download="activeDownload"
        :active-download-percent="activeDownloadPercent"
        @toggle="toggleQueuePanel"
        @clear-finished="clearFinished"
        @cancel-all="cancelAll"
        @cancel="cancelItem"
        @remove="removeItem"
        @retry="retryItem"
      />

      <template v-if="!isMobile">
        <div
          class="desktop-content-layout"
          :class="{
            'has-details': detailsVisible,
          }"
        >
          <div class="desktop-list-primary-shell">
            <!--
              批量操作条属于列表列：宽度与列表一致（不会压到详情面板），
              且吸顶范围覆盖整个列表列——长列表滚动时始终可达。
              放在工具栏内会被工具栏高度限制住，放在 layout 外层则会横跨详情列。
            -->
            <BatchActionBar
              v-if="(batchMode || selectedCount > 0) && canWrite"
              :selected-count="selectedCount"
              :can-write="canWrite"
              :can-transfer="!isRestricted"
              @select-all="selectAllVisible"
              @clear-selection="clearSelection"
              @download="batchDownload"
              @move="batchMove"
              @rename="renameSelected"
              @delete="batchDelete"
            />

            <div
              class="desktop-list-shell"
              :class="{ 'is-density-compact': fileView.density === 'compact' }"
              role="region"
              aria-label="文件列表"
              tabindex="-1"
            >
              <FileBrowserDesktopContent
                :loading="loading"
                :error="error"
                :search-active="searchActive"
                :search-mode="searchMode"
                :search-results="searchResults"
                :desktop-items="desktopItems"
                :files="files"
                :show-library-cards="showLibraryCards"
                :library-directories="libraryDirectories"
                :can-write="canWrite"
                :can-share="canShare"
                :view-mode="viewMode"
                :batch-mode="batchMode"
                :selected-paths="selectedPaths"
                :flash-path="dropFlashPath"
                :expanded-path="expandedFilePath"
                :active-path="desktopActivePath"
                :details-visible="detailsVisible"
                :shared-paths="sharedPaths"
                :sort-field="fileView.sortField"
                :sort-direction="fileView.sortDirection"
                :thumbnail-size="fileView.thumbnailSize"
                :renaming-path="renamingPath"
                :commit="browseCommit"
                :result-filter="resultFilter"
                :search-query="searchQuery"
                @retry="refresh"
                @upload-files="openUploader('files')"
                @create-folder="promptCreateDirectory(currentPath || '')"
                @switch-to-name-search="switchToNameSearch"
                @clear-search="clearSearch"
                @click="handleItemClick"
                @download="handleDownload"
                @rename="handleRenameEntry"
                @rename-commit="commitRenameEntry"
                @rename-cancel="cancelRenameEntry"
                @move="handleMoveEntry"
                @delete="handleDelete"
                @view-history="handleViewHistory"
                @toggle-select="handleRowSelect"
                @modifier-select="handleModifierSelect"
                @context-menu="handleContextMenu"
                @drag-start="handleDragStart"
                @drag-end="handleDragEnd"
                @drop-on-folder="handleDropOnFolder"
                @toggle-select-all="toggleSelectAll"
                @sort-change="handleSortChange"
                @share="handleShare"
                @preview="handlePreview"
                @open-folder="handleOpenFolder"
                @create-directory="handleCreateDirectory"
                @open-details="openDetailsDialog"
                @update:filter="setResultFilter"
              >
                <template #load-more>
                  <div
                    v-if="hasMore"
                    :ref="setLoadMoreSentinel"
                    class="desktop-load-more has-text-centered vf-text-muted is-size-7 py-3"
                  >
                    <span v-if="searchLoadingMore || filesStore.loadingMoreFiles"
                      >正在加载更多...</span
                    >
                    <span v-else-if="filesStore.loadMoreError">
                      {{ filesStore.loadMoreError }}
                      <button
                        class="button is-small is-light ml-2"
                        @click="filesStore.loadMoreFiles()"
                      >
                        重试
                      </button>
                    </span>
                    <span v-else
                      >继续下滑加载更多（已显示 {{ desktopItems.length }} /
                      {{ knownTotal }}）...</span
                    >
                  </div>
                </template>
              </FileBrowserDesktopContent>
            </div>
          </div>

          <FileBrowserDetailsPanel
            :visible="detailsVisible"
            :item="detailItem"
            :commit="browseCommit"
            :selection="selectedItems"
            :can-write="canWrite"
            :can-share="canShare"
            :can-transfer="!isRestricted"
            @close="fileView.toggleDetails()"
            @preview="handlePreview"
            @open-folder="handleOpenFolder"
            @download="handleDownload"
            @share="handleShare"
            @download-selection="batchDownload"
            @move-selection="batchMove"
            @transfer-selection="openTransferDialog(selectedItems)"
            @delete-selection="batchDelete"
            @select-all="selectAllVisible"
            @clear-selection="clearSelection"
            @view-history="handleViewHistory"
            @rename="handleRenameEntry"
            @move="handleMoveEntry"
            @transfer="openTransferDialog([$event])"
            @delete="handleDelete"
          />
        </div>

        <FileBrowserStatusBar
          :search-active="searchActive"
          :search-result-count="searchResults.length"
          :show-library-cards="showLibraryCards"
          :library-directory-count="libraryDirectories.length"
          :has-more-files="filesStore.hasMoreFiles"
          :files-count="files.length"
          :total-files="filesStore.totalFiles"
          :parent-path="parentPath"
          :selected-count="selectedCount"
          :type-ahead-prefix="typeAheadPrefix"
          @open-shortcuts="showShortcuts = true"
        />
      </template>

      <template v-else>
        <FileBrowserMobileContent
          :loading="loading"
          :error="error"
          :search-active="searchActive"
          :show-library-cards="showLibraryCards"
          :library-directories="libraryDirectories"
          :files="files"
          :visible-files="visibleFiles"
          :search-results="searchResults"
          :visible-search-results="visibleSearchResults"
          :search-query="searchQuery"
          :result-filter="resultFilter"
          :can-write="canWrite"
          :can-share="canShare"
          :view-mode="viewMode"
          :batch-mode="batchMode"
          :selected-paths="selectedPaths"
          :flash-path="dropFlashPath"
          :expanded-path="expandedFilePath"
          :thumbnail-size="fileView.thumbnailSize"
          :renaming-path="renamingPath"
          :commit="browseCommit"
          @retry="refresh"
          @click="handleItemClick"
          @download="handleDownload"
          @rename="handleRenameEntry"
          @rename-commit="commitRenameEntry"
          @rename-cancel="cancelRenameEntry"
          @move="handleMoveEntry"
          @delete="handleDelete"
          @view-history="handleViewHistory"
          @toggle-select="toggleSelect"
          @modifier-select="handleModifierSelect"
          @context-menu="handleContextMenu"
          @drag-start="handleDragStart"
          @drag-end="handleDragEnd"
          @drop-on-folder="handleDropOnFolder"
          @share="handleShare"
          @preview="handlePreview"
          @open-folder="handleOpenFolder"
          @create-directory="handleCreateDirectory"
          @open-details="openDetailsDialog"
          @update:filter="setResultFilter"
          @clear-search="clearSearch"
        >
          <template #load-more>
            <div
              v-if="isMobile && hasMore"
              :ref="setLoadMoreSentinel"
              class="has-text-centered vf-text-muted is-size-7 py-2"
            >
              <template v-if="searchLoadingMore || filesStore.loadingMoreFiles"
                >正在加载更多...</template
              >
              <template v-else-if="filesStore.loadMoreError">
                {{ filesStore.loadMoreError }}
                <button
                  class="button is-small is-light ml-2"
                  @click="filesStore.loadMoreFiles()"
                >
                  重试
                </button>
              </template>
              <template v-else>继续下滑加载更多...</template>
            </div>
          </template>
        </FileBrowserMobileContent>
      </template>
    </div>

    <!-- 上传对话框 -->
    <Modal
      :show="showUploader"
      title="上传文件"
      :mobile-compact="true"
      @close="closeUploader"
    >
      <FileUploader
        ref="fileUploaderRef"
        :existing-names="uploadExistingNames"
        :target-path="filesStore.currentPath"
        :initial-pick="uploaderPick"
        @upload="handleUpload"
        @close="showUploader = false"
      />
      <template #footer>
        <div class="buttons is-right">
          <button
            class="button"
            type="button"
            @click="fileUploaderRef?.cancelAll()"
            :disabled="!fileUploaderRef?.hasFiles"
          >
            取消全部
          </button>
          <button
            class="button"
            type="button"
            @click="showUploader = false"
            :disabled="fileUploaderRef?.uploading"
          >
            关闭
          </button>
          <button
            class="button is-primary"
            @click="fileUploaderRef?.startUpload()"
            :disabled="
              fileUploaderRef?.uploading || !fileUploaderRef?.hasQueued
            "
            :class="{ 'is-loading': fileUploaderRef?.uploading }"
          >
            <IconUpload :size="20" class="mr-2" />
            上传
          </button>
        </div>
      </template>
    </Modal>

    <!-- 目录管理对话框 -->
    <Modal
      :show="dirManagerOpen"
      title="目录管理"
      :mobile-compact="true"
      @close="dirManagerOpen = false"
    >
      <div class="content">
        <h3 class="title is-6">添加子目录</h3>
        <div class="field has-addons">
          <div class="control is-expanded">
            <input
              v-model="newDirName"
              class="input"
              type="text"
              aria-label="目录名"
              placeholder="目录名"
            />
          </div>
          <div class="control">
            <button
              class="button is-primary"
              :disabled="!newDirName.trim() || dirOpBusy"
              :class="{ 'is-loading': dirOpLoading === 'create' }"
              @click="createSubDir"
            >
              添加
            </button>
          </div>
        </div>

        <hr />

        <h3 class="title is-6">重命名当前目录</h3>
        <p v-if="!currentPath" class="vf-text-muted is-size-7">
          根目录不可重命名
        </p>
        <div class="field has-addons">
          <div class="control is-expanded">
            <input
              v-model="renameDirName"
              class="input"
              type="text"
              aria-label="新目录名"
              placeholder="新目录名"
              :disabled="!currentPath"
            />
          </div>
          <div class="control">
            <button
              class="button is-warning"
              :disabled="!currentPath || !renameDirName.trim() || dirOpBusy"
              :class="{ 'is-loading': dirOpLoading === 'rename' }"
              @click="renameCurrentDir"
            >
              重命名
            </button>
          </div>
        </div>

        <hr />

        <h3 class="title is-6">删除当前目录</h3>
        <p v-if="!currentPath" class="vf-text-muted is-size-7">
          根目录不可删除
        </p>
        <button
          class="button is-danger"
          :disabled="!currentPath || dirOpBusy"
          :class="{ 'is-loading': dirOpLoading === 'delete' }"
          @click="deleteCurrentDir"
        >
          删除当前目录
        </button>
      </div>
    </Modal>

    <MoveDialog
      :is-active="showMoveDialog"
      :items="moveDialogItems"
      :initial-path="moveDialogInitialPath"
      :confirm-loading="moveDialogSubmitting"
      :roots="isRestricted ? scopeRoots : []"
      @close="closeMoveDialog"
      @confirm="submitMoveDialog"
    />

    <ContextMenu
      :show="contextMenu.show"
      :x="contextMenu.x"
      :y="contextMenu.y"
      :items="contextMenuItems"
      @select="handleContextMenuSelect"
      @close="closeContextMenu"
    />

    <UploadDropOverlay
      :visible="externalDropActive"
      :target-label="dropTargetLabel"
    />

    <FileBrowserDialogs
      :ref="setDialogsRef"
      :show-history="showHistory"
      :show-share="showShareDialog"
      :show-details="showDetailsDialog"
      :show-transfer="showTransferDialog"
      :show-shortcuts="showShortcuts"
      :selected-file="selectedFile"
      :details-file="detailsDialogFile"
      :transfer-items="transferItems"
      :can-write="canWrite"
      :preview="preview"
      :preview-filename="previewFilename"
      :can-go-prev="canGoPrev"
      :can-go-next="canGoNext"
      :preview-position="previewIndex + 1"
      :preview-total="previewTotal"
      :can-copy-preview="canCopyPreview"
      :copy-state="previewCopyState"
      :preview-file="previewFile"
      @close-history="showHistory = false"
      @close-share="closeShareAndRefresh"
      @close-details="showDetailsDialog = false"
      @details-preview="onDetailsPreview"
      @details-download="handleDownload"
      @details-share="handleShare"
      @close-transfer="closeTransferDialog"
      @transfer="submitTransfer"
      @close-shortcuts="showShortcuts = false"
      @close-preview="closePreview"
      @retry-preview="openPreview"
      @prev-preview="prevPreview"
      @next-preview="nextPreview"
      @copy-preview="copyPreviewContent"
      @download-preview="downloadPreviewedFile"
      @reveal-preview="revealPreviewedFile"
    />


  </div>
</template>

<script setup lang="ts">
import {
  ref,
  onMounted,
  onBeforeUnmount,
  computed,
  watch,
} from "vue";
import { storeToRefs } from "pinia";
import { IconAlertCircle, IconUpload } from "@tabler/icons-vue";
import { filesService } from "../../services/files.service";
import { useFilesStore } from "../../stores/files.store";
import { useAppStore } from "../../stores/app.store";
import { useAuthStore } from "../../stores/auth.store";
import { useFileViewStore } from "../../stores/fileView.store";
import BatchActionBar from "./BatchActionBar.vue";
import FileBrowserDialogs from "./FileBrowserDialogs.vue";
import FileBrowserHistoryControls from "./FileBrowserHistoryControls.vue";
import FileBrowserViewControls from "./FileBrowserViewControls.vue";
import FileBrowserToolbarLeading from "./FileBrowserToolbarLeading.vue";
import FileBrowserToolbarHeading from "./FileBrowserToolbarHeading.vue";
import FileBrowserStatusBar from "./FileBrowserStatusBar.vue";
import FileBrowserDetailsPanel from "./FileBrowserDetailsPanel.vue";
import FileBrowserMobileContent from "./FileBrowserMobileContent.vue";
import FileBrowserDesktopContent from "./FileBrowserDesktopContent.vue";
import UploadDropOverlay from "./UploadDropOverlay.vue";
import MobileSearchBar from "./MobileSearchBar.vue";
import BrowserSearchBox, { type SearchType } from "./BrowserSearchBox.vue";
import DownloadQueuePanel from "./DownloadQueuePanel.vue";
import ContextMenu from "./ContextMenu.vue";
import MoveDialog from "./MoveDialog.vue";
import FileUploader from "../file-uploader/FileUploader.vue";
import Modal from "../common/Modal.vue";
import { useDownloadQueue } from "../../composables/useDownloadQueue";
import { useFilePreview } from "../../composables/useFilePreview";
import {
  useFileSearch,
  type SearchResultFilter,
} from "../../composables/useFileSearch";
import { useDirectoryNavigation } from "../../composables/useDirectoryNavigation";
import { useFileRename } from "../../composables/useFileRename";
import { useFileActions } from "../../composables/useFileActions";
import { useFileUpload } from "../../composables/useFileUpload";
import { useFileTransfer } from "../../composables/useFileTransfer";
import { useDragChip } from "../../composables/useDragChip";
import { useFileContextMenu } from "../../composables/useFileContextMenu";
import { useFileBrowserView } from "../../composables/useFileBrowserView";
import { useProgressiveList } from "../../composables/useProgressiveList";
import { usePreviewActions } from "../../composables/usePreviewActions";
import { useFileBrowserHotkeys } from "../../composables/useFileBrowserHotkeys";
import { useSharedPaths } from "../../composables/useSharedPaths";
import { useFileBrowserRowNavigation } from "../../composables/useFileBrowserRowNavigation";
import { useFileBrowserList } from "../../composables/useFileBrowserList";
import { useDirectoryManager } from "../../composables/useDirectoryManager";
import { useFileSelection } from "../../composables/useFileSelection";
import { useWindowFileDrop } from "../../composables/useWindowFileDrop";
import { useMoveDialog } from "../../composables/useMoveDialog";
import { useTouchGestures } from "../../composables/useTouchGestures";
import { useFilesSidebarControl } from "../../composables/useFilesSidebar";
import type { FileInfo } from "../../types";
import {
  firstScopeRoot,
  isPathWithinRoots,
  normalizeScopePath,
  sliceBreadcrumbsFromScopeRoot,
} from "../../utils/sessionScope";

const filesStore = useFilesStore();
const appStore = useAppStore();
const authStore = useAuthStore();
const fileView = useFileViewStore();
const { files, loading, error, currentPath, browseCommit } =
  storeToRefs(filesStore);

/**
 * 版本切换控件由页面（Home）持有目录历史状态，FileBrowser 只负责在
 * 面包屑行右侧渲染按钮并上报动作；移动端仍由页面底栏承担。
 */
withDefaults(
  defineProps<{
    historyLoading?: boolean;
    historyError?: boolean;
    historyStateLabel?: string;
    historyTitle?: string;
    historyHasCommit?: boolean;
  }>(),
  {
    historyLoading: false,
    historyError: false,
    historyStateLabel: "",
    historyTitle: "",
    historyHasCommit: false,
  },
);

const emit = defineEmits<{
  (e: "history-fast-back"): void;
  (e: "history-step-back"): void;
  (e: "history-step-forward"): void;
  (e: "history-fast-forward"): void;
}>();

/** 受限会话的目录范围与写权限（主密码会话恒为全量）。 */
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
    ? sliceBreadcrumbsFromScopeRoot(filesStore.breadcrumbs, scopeRoots.value)
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

const { sharedPaths, loadSharedPaths } = useSharedPaths(
  currentPath,
  () => filesService.getSharedPaths(),
);

// 目录级动态标题（W3C 标准：浏览位置进标题）
watch(currentPath, (path) => {
  const dir = path.split("/").filter(Boolean).pop();
  document.title = dir ? `${dir} - VFiles` : "VFiles - 文件管理系统";
});

// 事件驱动式（r138 ✓ 上轮计划兑现）：挂钩 navigateTo 单点（不依赖 watch 触发时序）
// + 重试式恢复（渲染竞态防 ✗ 双 rAF 后校验补设）。

const {
  isMobile,
  viewMenuOpen,
  sortState,
  viewMode,
  handleSortChange,
} = useFileBrowserView();

const searchContentEnabled = computed(
  () => authStore.features?.searchContent ?? false,
);

/**
 * 视图侧栏开关由 FilesLayout 通过 provide/inject 提供（桌面收起、移动抽屉）。
 * 没有 FilesLayout 时（组件测试 / 独立嵌入）为 null，工具栏不渲染汉堡按钮。
 */
const sidebarControl = useFilesSidebarControl();
const sidebarVisible = computed(() => sidebarControl?.isVisible.value ?? false);

function toggleSidebar() {
  sidebarControl?.toggle();
}

const showShortcuts = ref(false);
/** 工具栏「+ 新建 → 新建文件夹」：复用移动端底栏的建目录请求流程。 */
function requestCreateDirectoryFromToolbar() {
  if (!canWrite.value) return;
  appStore.requestCreateDirectory();
}

const expandedFilePath = ref<string>("");

const {
  preview,
  previewFilename,
  previewIndex,
  previewTotal,
  canGoPrev,
  canGoNext,
  prevPreview,
  nextPreview,
  closePreview,
  openPreview,
} = useFilePreview(browseCommit, {
  // 当前视图中的文件（不含目录与 `.`/`..`），用于预览的上一张/下一张
  getPreviewableFiles: () => previewableFiles.value,
});

/**
 * 右侧「详细信息」面板：桌面端显示，移动端隐藏。
 * 展示当前活动条目（键盘高亮或唯一选中项，见 findActiveItem）。
 */
const detailsVisible = computed(
  () => !isMobile.value && fileView.detailsVisible,
);

/** 移动端搜索筛选面板（默认收起，保持顶部只有一行搜索框）。 */
const mobileSearchFiltersOpen = ref(false);

/**
 * 整窗拖放上传：把桌面文件直接拖进浏览器。
 *
 * 已有弹窗（预览/上传/移动/历史/分享）时交给弹窗自己处理，不弹整窗提示。
 */
const anyDialogOpen = computed(
  () =>
    preview.value.open ||
    showUploader.value ||
    showHistory.value ||
    showShareDialog.value ||
    showMoveDialog.value ||
    dirManagerOpen.value ||
    showDetailsDialog.value,
);
const dropTargetLabel = computed(() =>
  filesStore.currentPath
    ? filesStore.currentPath.split("/").filter(Boolean).pop() ||
      filesStore.currentPath
    : "根目录",
);

const { dragging: externalDropActive } = useWindowFileDrop({
  onFiles: (dropped) => queueDroppedFiles(dropped),
  enabled: () => !anyDialogOpen.value && canWrite.value,
});

/** 预览导航使用的文件列表，跟随当前视图（目录或搜索结果）。 */
const previewableFiles = computed<FileInfo[]>(() =>
  (searchActive.value
    ? sortedSearchResults.value
    : navigationListItems.value
  ).filter((file) => file.kind === "file"),
);

const {
  resultFilter,
  setResultFilter,
  searchQuery,
  searchResults,
  searchResultsMayBeIncomplete,
  searchLoading,
  searchError,
  searchActive,
  searchContent,
  searchType,
  searchScopeCurrent,
  desktopSearchOpen,
  desktopSearchBoxRef,
  desktopSearchInputRef,
  searchHistory,
  searchMode,
  desktopSearchFiltersActive,
  searchHasMore,
  searchLoadingMore,
  loadMoreSearchResults,
  closeDesktopSearch,
  clearSearch,
  runSearch,
  doSearch,
  runDesktopSearch,
  clearDesktopSearch,
} = useFileSearch(currentPath);

/** 把模板中的高级搜索输入框写回 composable 的 ref（供聚焦使用）。 */
function setDesktopSearchInput(el: Element | { $el?: Element } | null) {
  const element = el instanceof HTMLInputElement ? el : null;
  desktopSearchInputRef.value = element;
}

const {
  previewCopyState,
  canCopyPreview,
  previewFile,
  downloadPreviewedFile,
  revealPreviewedFile,
  copyPreviewContent,
} = usePreviewActions({
  preview,
  files,
  searchResults,
  closePreview,
  searchActive,
  clearSearch,
  loadFiles: (path) => filesStore.loadFiles(path),
  setActivePath: (path) => {
    desktopActivePath.value = path;
  },
  handleDownload,
});

const {
  navigateTo,
  handleBreadcrumbNavigate,
  refresh,
  goBack,
  goRoot,
  parentPath,
} = useDirectoryNavigation({
  currentPath,
  isRestricted,
  scopeRoots,
  searchActive,
  clearSearch,
  expandedFilePath,
  getSelectedCount: () => selectedCount.value,
  clearSelection: () => clearSelection(),
  navigateStore: (path) => filesStore.navigateTo(path),
  reload: (path) => filesStore.loadFiles(path),
  errorStatus: computed(() => filesStore.errorStatus),
  requestedPath: computed(() => filesStore.requestedPath),
  notifyError: (message) => appStore.error(message),
});

const {
  dirManagerOpen,
  dirOpLoading,
  dirOpBusy,
  newDirName,
  renameDirName,
  refreshAfterMutation,
  renameEntryPath,
  promptCreateDirectory,
  createSubDir,
  renameCurrentDir,
  deleteCurrentDir,
} = useDirectoryManager({
  currentPath,
  searchActive,
  clearSearch,
  navigateTo,
  refresh,
  doSearch,
  setActivePath: (path) => {
    desktopActivePath.value = path;
  },
});

const desktopActivePath = ref("");

const {
  showMoveDialog,
  moveDialogItems,
  moveDialogInitialPath,
  moveDialogSubmitting,
  openMoveDialog,
  closeMoveDialog,
  openMoveForEntry,
  submitMoveDialog,
  moveEntryToDirectory,
} = useMoveDialog({
  currentPath,
  refreshAfterMutation,
  // 选择相关的 helper 在下方 useFileSelection 中定义，这里延迟调用即可
  replaceSelectedPath: (oldPath, newPath) =>
    replaceSelectedPath(oldPath, newPath),
  getActivePath: () => desktopActivePath.value,
  setActivePath: (path) => {
    desktopActivePath.value = path;
  },
  clearSelection: () => clearSelection(),
});

const {
  batchMode,
  selectedPaths,
  lastSelectedPath,
  selectedCount,
  toggleBatchMode,
  toggleSelect,
  handleModifierSelect,
  clearSelection,
  replaceSelectedPath,
  narrowSelectionTo,
  selectAllVisible,
  toggleSelectAll,
  batchDownload,
  batchDelete,
  batchMove,
  renameSelected,
} = useFileSelection({
  searchActive,
  getVisibleItems: () =>
    searchActive.value ? sortedSearchResults.value : navigationListItems.value,
  getSelectionPool: () =>
    searchActive.value ? searchResults.value : files.value,
  setActivePath: (path) => {
    desktopActivePath.value = path;
  },
  currentPath,
  browseCommit,
  refresh,
  doSearch,
  openMoveDialog,
  renameEntry: (file) => handleRenameEntry(file),
  enqueueDownload: (kind, path) => enqueueDownload(kind, path),
});

const {
  renamingPath,
  handleRenameEntry,
  commitRenameEntry,
  cancelRenameEntry,
} = useFileRename({
  canWrite,
  desktopActivePath,
  replaceSelectedPath,
  renameEntryPath,
  refreshAfterMutation,
  notifySuccess: (message) => appStore.success(message),
  notifyError: (message) => appStore.error(message),
});

const {
  selectedFile,
  detailsDialogFile,
  showHistory,
  showShareDialog,
  showDetailsDialog,
  handleDelete,
  handleCreateDirectory,
  createDirectoryHere,
  handleMoveEntry,
  handleViewHistory,
  handleShare,
  closeShareDialog,
  openDetailsDialog,
} = useFileActions({
  canWrite,
  canShare,
  currentPath,
  searchActive,
  desktopActivePath,
  deletePath: (path, detail) => filesStore.deleteFile(path, detail),
  promptCreateDirectory,
  openMoveForEntry,
  doSearch,
  notifySuccess: (message) => appStore.success(message),
  notifyError: (message) => appStore.error(message),
});

const {
  showUploader,
  uploaderPick,
  fileUploaderRef,
  uploadExistingNames,
  uploadIndicator,
  openUploader,
  closeUploader,
  queueDroppedFiles,
  handleUpload,
} = useFileUpload({
  canWrite,
  files,
  refresh,
  notifySuccess: (message) => appStore.success(message),
});

const {
  showTransferDialog,
  transferItems,
  setDialogsRef,
  openTransferDialog,
  closeTransferDialog,
  submitTransfer,
} = useFileTransfer({
  isRestricted,
  clearSelection,
  refresh,
  notifySuccess: (message) => appStore.success(message),
  notifyError: (message) => appStore.error(message),
});

const {
  contextMenu,
  contextMenuItems,
  openRowContextMenu,
  handleContextMenu,
  closeContextMenu,
  handleContextMenuSelect,
} = useFileContextMenu({
  files,
  canWrite,
  canShare,
  isRestricted,
  batchMode,
  selectedPaths,
  desktopActivePath,
  narrowSelectionTo,
  toggleSelect,
  openDetailsDialog,
  handleOpenFolder,
  handleCreateDirectory,
  handlePreview,
  handleViewHistory,
  handleRenameEntry,
  handleMoveEntry,
  openTransferDialog,
  handleDownload,
  handleShare,
  handleDelete,
  notifySuccess: (message) => appStore.success(message),
  notifyError: (message) => appStore.error(message),
});

const {
  draggingFile,
  dragChipPos,
  dragChipTarget,
  dragChipInvalid,
  dragChipLabel,
  dropFlashPath,
  handleDropOnFolder,
  handleDragStart,
  handleDragEnd,
} = useDragChip(selectedPaths, {
  canWrite,
  moveEntryToDirectory,
});

const {
  queueCollapsed,
  downloadQueue,
  downloading,
  activeDownload,
  activeDownloadPercent,
  enqueueDownload,
  toggleQueuePanel,
  cancelItem,
  cancelAll,
  clearFinished,
  removeItem,
  retryItem,
} = useDownloadQueue(browseCommit);

/** 受限会话进入时，范围外的路径（或空路径）回退到第一个范围根。 */
function resolvedEntryPath(): string {
  const current = normalizeScopePath(filesStore.currentPath);
  if (!isRestricted.value) return current;
  const roots = scopeRoots.value;
  if (roots.length === 0) return current;
  return isPathWithinRoots(current, roots)
    ? current
    : firstScopeRoot(roots);
}

onMounted(() => {
  filesStore.loadFiles(resolvedEntryPath());
});

onBeforeUnmount(() => {
  closePreview();
});

const {
  navigationListItems,
  showLibraryCards,
  libraryDirectories,
  sortedSearchResults,
  activeList,
  knownTotal,
} = useFileBrowserList({
  files,
  sortState,
  searchActive,
  searchResults,
  resultFilter,
  currentPath,
  browseCommit,
  libraryCards: computed(() => fileView.libraryCards),
  loading,
  error,
  isRestricted,
  scopeRoots,
  totalFiles: computed(() => filesStore.totalFiles),
});

const {
  visibleCount,
  hasMore,
  setLoadMoreSentinel,
} = useProgressiveList({
  activeList,
  isMobile,
  searchActive,
  searchHasMore,
  loadMoreSearchResults,
  serverHasMore: computed(() => filesStore.hasMoreFiles),
  loadMoreServer: () => filesStore.loadMoreFiles(),
  resetKey: computed(
    () =>
      `${filesStore.currentPath}|${searchActive.value}|${searchQuery.value}|${searchResults.value.length}`,
  ),
});

const visibleFiles = computed(() =>
  navigationListItems.value.slice(0, visibleCount.value),
);
const visibleSearchResults = computed(() =>
  sortedSearchResults.value.slice(0, visibleCount.value),
);
const desktopItems = computed(() =>
  activeList.value.slice(0, visibleCount.value),
);

const {
  findActiveItem,
  moveActiveRow,
  typeAheadPrefix,
  clearTypeAhead,
  handleTypeAhead,
  toggleActiveRowSelection,
  handleRowSelect,
} = useFileBrowserRowNavigation({
  searchActive,
  sortedSearchResults,
  navigationListItems,
  activeList,
  visibleCount,
  viewMode,
  desktopActivePath,
  lastSelectedPath,
  selectedPaths,
  batchMode,
  canWrite,
  toggleSelect,
  handleModifierSelect,
});

const detailItem = computed<FileInfo | undefined>(() => findActiveItem());

/** 当前批量选中的条目（保持列表顺序），用于详情面板的多选摘要。 */
const selectedItems = computed<FileInfo[]>(() =>
  activeList.value.filter((file) => selectedPaths.value.has(file.path)),
);

// 文件列表重新加载（上传/删除/重命名/移动/切换目录）后刷新侧栏概览
// 移动端底栏的「新建文件夹」：在当前位置打开创建对话框
watch(
  () => appStore.createDirectoryRequests,
  () => {
    if (!canWrite.value) return;
    void createDirectoryHere();
  },
);

watch(
  [isMobile, desktopItems, currentPath],
  () => {
    if (isMobile.value) {
      desktopActivePath.value = "";
      return;
    }

    if (desktopItems.value.length === 0) {
      desktopActivePath.value = "";
      return;
    }

    const current = desktopItems.value.find(
      (file) => file.path === desktopActivePath.value,
    );
    // 目录刚加载时还没有活动行：默认选中第一个条目（用户已显式选择时不打扰）。
    if (
      !current &&
      selectedPaths.value.size === 0 &&
      desktopItems.value.length > 0
    ) {
      desktopActivePath.value = desktopItems.value[0].path;
    }
  },
  { immediate: true },
);

const {
  pullReady,
  pullRefreshing,
  pullIndicatorVisible,
  onTouchStart,
  onTouchMove,
  onTouchEnd,
} = useTouchGestures({
  enabled: isMobile,
  isBlocked: anyOverlayOpen,
  refresh,
  goBack,
});

function handleItemClick(file: FileInfo) {
  if (file.kind === "directory") {
    handleOpenFolder(file);
    return;
  }

  if (!isMobile.value && !batchMode.value) {
    desktopActivePath.value = file.path;
    // 记录锚点，便于随后 Shift 点击做范围选择
    lastSelectedPath.value = file.path;
    // 点击行后把焦点交给列表容器：行本身不可聚焦，若焦点仍停在工具栏按钮上，
    // Shift+F10 / 方向键等列表快捷键会被「交互控件」守卫挡掉。
    document
      .querySelector<HTMLElement>(".desktop-list-shell")
      ?.focus({ preventScroll: true });
    return;
  }

  if (expandedFilePath.value === file.path) {
    expandedFilePath.value = "";
  } else {
    expandedFilePath.value = file.path;
  }
}

function handlePreview(file: FileInfo) {
  openPreview(file.path);
}

/** 详细信息弹窗里的「预览」：关掉详情再打开预览。 */
function onDetailsPreview(file: FileInfo) {
  showDetailsDialog.value = false;
  handlePreview(file);
}

function handleOpenFolder(file: FileInfo) {
  if (file.kind === "directory") {
    if (searchActive.value) {
      clearSearch();
    }
    navigateTo(file.path);
  }
}

function handleDownload(file: FileInfo) {
  // Route downloads through the queue so users get progress, error feedback,
  // and cancellation support instead of a silent browser download.
  enqueueDownload(file.kind === "directory" ? "folder" : "file", file.path);
}

/** 快速预览（空格 ✓ 云盘 web 惯例 = Finder Quick Look 同款）：预览活动行。 */
function quickLook() {
  const active = findActiveItem();
  if (active) openPreview(active.path);
}


/** 搜索框 ↓ 键：进入结果列表首项（活动项模型，容器级 ↑↓ 随后自然接管 ✓） */
function enterSearchResults() {
  const first = searchResults.value[0];
  if (!first) return;
  desktopActivePath.value = first.path;
}

defineExpose({
  // 移动端底栏的「上传」：直接弹出文件选择器（与工具栏主按钮一致）
  openUploader: () => openUploader("files"),
  refresh,
  goBack,
  goRoot,
  toggleBatchMode,
  setSearchQuery: (q: string) => {
    searchQuery.value = q;
  },
  runSearch,
  clearSearch,
  batchMode,
  selectedCount,
  selectAllVisible,
  clearSelection,
  batchDownload,
  batchDelete,
  batchMove,
  renameSelected,
  searchLoading,
});

function anyOverlayOpen(): boolean {
  return (
    contextMenu.value.show ||
    preview.value.open ||
    showShortcuts.value ||
    showUploader.value ||
    showHistory.value ||
    showShareDialog.value ||
    showMoveDialog.value ||
    showDetailsDialog.value ||
    showTransferDialog.value ||
    dirManagerOpen.value ||
    // 全局确认框也盖住列表：弹着时 Delete/F2 不能穿透到后台行。
    // 用 DOM 判定而不是 dialogState，避免上一个对话框未结算时的模块级残留。
    document.querySelector(".modal.is-active") !== null
  );
}

/** 当前键盘操作的目标：优先高亮行，其次唯一的已选条目。 */
/**
 * 行内复选框：即使不在「批量选择」模式，勾选一项也应立刻出现操作条
 * （与主流网盘一致：勾选即进入选择态，不必先点工具栏按钮）。
 */
/** 分享对话框关闭后强制刷新徽标集合（期间可能新建或停止分享）。 */
function closeShareAndRefresh() {
  closeShareDialog();
  void loadSharedPaths(true);
}

/** 内容搜索无结果时，一键改用文件名搜索（保留关键字与范围）。 */
function switchToNameSearch() {
  searchContent.value = false;
  void runDesktopSearch();
}

useFileBrowserHotkeys({
  viewMenuOpen,
  typeAheadPrefix,
  clearTypeAhead,
  desktopSearchOpen,
  closeDesktopSearch,
  previewOpen: computed(() => preview.value.open),
  closePreview,
  prevPreview,
  nextPreview,
  showShortcuts,
  batchMode,
  toggleBatchMode,
  selectedPaths,
  clearSelection,
  canWrite,
  selectAllVisible,
  batchDelete,
  findActiveItem,
  handleDelete,
  moveActiveRow,
  toggleActiveRowSelection,
  quickLook,
  handleTypeAhead,
  openRowContextMenu,
  renameSelected,
  handleRenameEntry,
  handleOpenFolder,
  handlePreview,
  anyOverlayOpen,
  desktopSearchBoxRef,
});

</script>

<style scoped>
.file-browser {
  /* 指向全局设计令牌，随明暗主题切换 */
  margin: 0 auto;
  padding: 0;
}

/* 单一内容面板：内部区块之间只用细分隔线，不再层层嵌套卡片 */
.file-browser-box {
  display: flex;
  flex-direction: column;
  border-radius: var(--vf-radius-card);
  border: 1px solid var(--vf-border-weak);
  background: var(--vf-surface);
  box-shadow: var(--vf-shadow-card);
}

/* 桌面端由外层应用外壳给定高度：盒子撑满剩余空间，内部列表自行滚动 */
@media screen and (min-width: 1024px) {
  .file-browser {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
  }

  .file-browser-box {
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
  }

  .desktop-content-layout {
    flex: 1 1 auto;
    min-height: 0;
    /* 单行铺满容器高度：否则 grid 行按内容撑开，列表无法内部滚动 */
    grid-template-rows: minmax(0, 1fr);
  }

  .desktop-list-primary-shell {
    min-height: 0;
  }

  .desktop-list-shell:focus {
  outline: none;
}

.desktop-list-shell {
    flex: 1 1 auto;
    min-height: 0;
    /* 列表区域是唯一滚动容器：行、加载更多提示一起滚动，
       列头用 sticky 常驻，与主流网盘一致 */
    overflow: auto;
    /* 不再用 scrollbar-gutter: stable —— 它会在无滚动条时也永久占用右侧
       滚动条宽度，使表头/行/表尾比工具栏窄一截（右边缘对不齐）。
       滚动条出现时列表按浏览器行为收窄，是主流网盘的一致表现。 */
    /* sticky 表头的滚动缓冲（44px 表头高）：scrollIntoView/键盘导航停在表头下方，
       否则向上导航时活动行会被吸进表头。 */
    scroll-padding-top: var(--vf-table-head-height);
  }

  /* thead 由子组件 FileList.vue 渲染，需 :deep() 才能匹配到 th（此前规则从未生效） */
  .desktop-list-shell :deep(thead th) {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--vf-surface-2);
  }
}

/* ---- 合并后的单行文件工具栏（50px）：面包屑行 + 命令条合成一行 ---- */
.breadcrumb-bar.file-toolbar {
  position: relative;
  z-index: 2;
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  flex-wrap: nowrap;
  height: var(--vf-toolbar-height);
  flex: 0 0 auto;
  /* 与表格单元格、表尾同一 16px 基线 */
  padding: 0 var(--vf-cell-padding-x);
  border-bottom: 1px solid var(--vf-border-weak);
  background: var(--vf-surface);
}

/* 左组：汉堡按钮 / 「+ 新建」/ 范围徽标 / 视图名或面包屑 */
.file-toolbar-leading {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  flex: 1 1 auto;
  min-width: 0;
}

/* 面包屑：灰色小字，当前层深色（覆盖 Breadcrumb 默认的强调色） */
.breadcrumb-bar.file-toolbar :deep(.path-bar) {
  min-height: 0;
  flex: 0 1 auto;
  min-width: 0;
}

.file-toolbar :deep(.path-bar-segment) {
  padding: 4px 6px;
  color: var(--vf-text-muted);
  font-size: var(--vf-text-sm);
  font-weight: 500;
}

.file-toolbar :deep(.path-bar-item.is-active .path-bar-segment) {
  color: var(--vf-text);
  font-weight: 600;
}

.file-toolbar :deep(.path-bar-toggle) {
  margin-left: 0;
}

/* 右组：版本胶囊 / 搜索 / 列表网格切换 / 排序 / 详细信息 */
.file-toolbar-actions {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  margin-left: auto;
  flex: 0 1 auto;
  min-width: 0;
}

.file-toolbar-actions :deep(.desktop-search-box) {
  margin-left: 0;
}

.file-toolbar-error {
  margin: var(--vf-space-2) var(--vf-cell-padding-x) 0;
}

/* 移动端搜索行（桌面已合并进单行工具栏） */
.file-browser-toolbar {
  padding: 0.55rem var(--vf-space-3) 0.6rem;
  border-bottom: 1px solid var(--vf-border-weak);
}

.desktop-list-primary-shell {
  display: flex;
  flex-direction: column;
  position: relative;
  z-index: 0;
  min-height: 0;
}

.desktop-list-shell {
  min-width: 0;
}

.desktop-list-shell {
  display: flex;
  flex-direction: column;
}

/* separate + 0 间距：sticky 表头在 collapse 下边框渲染有已知怪癖；
   行染色满铺无圆角，仅水平分隔线，视觉与 collapse 一致。 */
.desktop-list-shell table {
  border-collapse: separate;
  border-spacing: 0;
}

.desktop-list-meta {
  margin-bottom: 0.7rem;
  color: var(--vf-text-muted);
  font-size: var(--vf-text-sm);
}

/* 状态栏退化为一行低调的说明文字 */
/* 桌面内容区：列表 + 右侧详细信息面板 */
.desktop-content-layout {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  min-height: 0;
}

/* ---- Seafile 式外壳：视图侧栏在页面级，文件区为全高、无浮动卡片 ----
   padding: 0 覆盖 Bulma .box 的 1.25rem：工具栏/表格/表尾各自持有 16px，
   这样内容左右边缘与其它页面落在同一条基线上。 */
.file-browser {
  height: 100%;
}

.file-browser-box {
  padding: 0;
  border: 0;
  border-radius: 0;
  box-shadow: none;
  background: var(--vf-surface);
  height: calc(100vh - var(--vf-topbar-height));
  height: calc(100dvh - var(--vf-topbar-height));
}


@media screen and (max-width: 1023px) {
  .file-browser-box {
    height: auto;
    min-height: calc(100vh - var(--vf-topbar-height));
  min-height: calc(100dvh - var(--vf-topbar-height));
  }
}

/* 详情列宽度由 --vf-details-width 统一（350px），与列表/详情栏保持同步 */
.desktop-content-layout.has-details {
  grid-template-columns: minmax(0, 1fr) var(--vf-details-width);
}

@media screen and (max-width: 1023px) {
  .file-browser {
    padding: 0;
  }

  .file-browser-box {
    padding: var(--vf-space-2);
    border-radius: 0;
    box-shadow: none;
    border-left: none;
    border-right: none;
  }

  .level {
    flex-direction: column;
    align-items: stretch !important;
  }

  .level-left,
  .level-right {
    width: 100%;
  }

  .level-item {
    margin-bottom: var(--vf-space-2);
  }

  .level .button {
    width: 100%;
    justify-content: center;
  }
}
.vf-hud-enter-active,
.vf-hud-leave-active {
  transition: opacity 0.2s var(--vf-motion-standard);
}

.vf-hud-enter-from,
.vf-hud-leave-to {
  opacity: 0;
}

.desktop-typeahead-hud {
  position: fixed;
  top: 42vh;
  left: 50%;
  transform: translateX(-50%);
  z-index: 60;
  padding: var(--vf-space-2) var(--vf-space-4);
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-card);
  background: var(--vf-surface-raised);
  box-shadow: var(--vf-shadow-menu);
  color: var(--vf-text);
  font-size: var(--vf-text-4xl);
  font-weight: 600;
  letter-spacing: 0.04em;
  pointer-events: none;
  transition: opacity 0.12s var(--vf-motion-standard);
}

.desktop-drag-chip.is-invalid {
  background: var(--vf-danger-soft);
  border-color: var(--vf-danger-line);
}

.desktop-drag-chip.is-over-target {
  background: var(--vf-accent-soft-strong);
  border-color: var(--vf-accent);
}

.desktop-drag-chip {
  position: fixed;
  z-index: 60;
  padding: 0.35rem var(--vf-space-3);
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface-raised);
  box-shadow: var(--vf-shadow-menu);
  color: var(--vf-text);
  font-size: var(--vf-text-sm);
  font-weight: 600;
  pointer-events: none;
  white-space: nowrap;
}

/* ---- Seafile 式外壳（最后一条规则，覆盖上面的桌面端卡片样式） ---- */
.file-browser {
  width: 100%;
  max-width: none;
  margin: 0;
  height: 100%;
}

.file-browser-box {
  border: 0;
  border-radius: 0;
  box-shadow: none;
  background: var(--vf-surface);
  width: 100%;
  max-width: none;
  margin: 0;
}

@media screen and (min-width: 1024px) {
  .file-browser-box {
    height: calc(100vh - var(--vf-topbar-height));
  height: calc(100dvh - var(--vf-topbar-height));
  }
}
</style>
