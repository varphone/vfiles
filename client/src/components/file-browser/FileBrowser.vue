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
      <div class="breadcrumb-bar">
        <Breadcrumb
          :breadcrumbs="filesStore.breadcrumbs"
          :directories="filesStore.directories"
          @navigate="handleBreadcrumbNavigate"
          @drop="handleDropOnFolder"
        />
      </div>

      <div class="file-browser-toolbar">
        <template v-if="isMobile">
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
        </template>

        <template v-else>
          <DesktopCommandBar
            :can-go-up="Boolean(currentPath)"
            :details-visible="fileView.detailsVisible"
            :batch-mode="batchMode"
            :search-query="searchQuery"
            :search-open="desktopSearchOpen"
            :search-content="searchContent"
            :search-type="searchType"
            :search-scope-current="searchScopeCurrent"
            :search-loading="searchLoading"
            :search-active="searchActive"
            :search-content-enabled="searchContentEnabled"
            :search-filters-active="desktopSearchFiltersActive"
            :search-history="searchHistory"
            :search-error="searchError"
            :upload-indicator="uploadIndicator"
            :register-input="setDesktopSearchInput"
            @go-up="goBack"
            @refresh="refresh"
            @toggle-details="fileView.toggleDetails()"
            @toggle-batch="toggleBatchMode"
            @upload-files="openUploader('files')"
            @upload-folder="openUploader('directory')"
            @create-folder="createDirectoryHere"
            @search="runDesktopSearch"
            @clear="clearDesktopSearch"
            @update:search-query="searchQuery = $event"
            @update:search-open="desktopSearchOpen = $event"
            @update:search-content="searchContent = $event"
            @update:search-type="searchType = $event as SearchType"
            @update:search-scope-current="searchScopeCurrent = $event"
          />

          <div
            v-if="searchError"
            class="notification is-danger is-light mb-3"
            role="alert"
          >
            <IconAlertCircle :size="20" class="mr-2" />
            {{ searchError }}
          </div>
        </template>
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
            'has-tree': treeVisible,
            'has-details': detailsVisible,
          }"
        >
          <aside v-if="treeVisible" class="browser-sidebar">
            <DirectoryTree
              :current-path="currentPath"
              :dragging="Boolean(draggingFile)"
              :refresh-key="sidebarVersion"
              @navigate="handleTreeNavigate"
              @drop-on-folder="handleDropOnFolder"
            />
            <SidebarOverview
              :refresh-key="sidebarVersion"
              @open-file="handleOpenRecentFile"
              @open-favorite="handleOpenFavorite"
              @favorites-changed="handleFavoritesChanged"
            />
          </aside>

          <div class="desktop-list-primary-shell">
            <!--
              批量操作条属于列表列：宽度与列表一致（不会压到详情面板），
              且吸顶范围覆盖整个列表列——长列表滚动时始终可达。
              放在工具栏内会被工具栏高度限制住，放在 layout 外层则会横跨详情列。
            -->
            <BatchActionBar
              v-if="batchMode || selectedCount > 0"
              :selected-count="selectedCount"
              @select-all="selectAllVisible"
              @clear-selection="clearSelection"
              @download="batchDownload"
              @move="batchMove"
              @rename="renameSelected"
              @delete="batchDelete"
            />

            <div class="desktop-list-shell">
              <FileSkeleton
                v-if="loading"
                :variant="viewMode === 'grid' ? 'grid' : 'list'"
                :thumbnail-size="fileView.thumbnailSize"
              />

              <EmptyState
                v-else-if="error"
                :icon="IconAlertCircle"
                tone="error"
                title="加载失败"
                :hint="error"
              >
                <template #actions>
                  <button
                    class="vf-ghost-button"
                    :class="{ 'is-loading': loading }"
                    :disabled="loading"
                    @click="refresh"
                  >
                    <IconRefresh :size="16" />
                    <span>重试</span>
                  </button>
                </template>
              </EmptyState>

              <EmptyState
                v-else-if="!searchActive && files.length === 0"
                :icon="IconFolderOpen"
                title="此文件夹为空"
                hint="拖拽文件到这里，或使用下面的按钮上传"
              >
                <template #actions>
                  <button
                    class="vf-ghost-button is-primary"
                    @click="openUploader('files')"
                  >
                    <IconUpload :size="16" />
                    <span>上传文件</span>
                  </button>
                  <button
                    class="vf-ghost-button"
                    @click="promptCreateDirectory(currentPath || '')"
                  >
                    <IconFolderPlus :size="16" />
                    <span>新建文件夹</span>
                  </button>
                </template>
              </EmptyState>

              <EmptyState
                v-else-if="searchActive && searchResults.length === 0"
                :icon="IconSearch"
                title="没有找到匹配的文件"
                :hint="
                  searchMode === 'content'
                    ? '换个关键字，或改用文件名搜索（内容搜索只匹配文件内容）'
                    : '换个关键字，或检查是否限制了搜索范围'
                "
              >
                <template #actions>
                  <button
                    v-if="searchMode === 'content'"
                    class="vf-ghost-button is-primary"
                    @click="switchToNameSearch"
                  >
                    <IconFileSearch :size="16" />
                    <span>改为文件名搜索</span>
                  </button>
                  <button class="vf-ghost-button" @click="clearSearch">
                    <span>清空搜索</span>
                  </button>
                </template>
              </EmptyState>

              <template v-else>
                <SearchResultToolbar
                  v-if="searchActive"
                  class="desktop-list-meta"
                  :results="searchResults"
                  :active="resultFilter"
                  :query="searchQuery"
                  :visible-count="desktopItems.length"
                  @update:active="setResultFilter"
                />

                <FileGrid
                  v-if="viewMode === 'grid'"
                  :files="desktopItems"
                  :highlight="searchActive ? searchQuery : ''"
                  :commit="browseCommit"
                  :select-mode="batchMode"
                  :selected-paths="selectedPaths"
                  :active-path="desktopActivePath"
                  :thumbnail-size="fileView.thumbnailSize"
                  :show-location="searchActive"
                  @click="handleItemClick"
                  @download="handleDownload"
                  @rename="handleRenameEntry"
                  :renaming-path="renamingPath"
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
                  @share="handleShare"
                  @preview="handlePreview"
                  @open-folder="handleOpenFolder"
                  @create-directory="handleCreateDirectory"
                />
                <FileList
                  v-else
                  :files="desktopItems"
                  :highlight="searchActive ? searchQuery : ''"
                  :show-location="searchActive"
                  :select-mode="batchMode"
                  :selected-paths="selectedPaths"
                  :flash-path="dropFlashPath"
                  :expanded-path="expandedFilePath"
                  :active-path="desktopActivePath"
                  :desktop="true"
                  :show-action-column="!detailsVisible"
                  :sort-field="fileView.sortField"
                  :sort-direction="fileView.sortDirection"
                  @click="handleItemClick"
                  @download="handleDownload"
                  @rename="handleRenameEntry"
                  :renaming-path="renamingPath"
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
                />

                <div
                  v-if="hasMore"
                  ref="loadMoreSentinel"
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
            </div>
          </div>

          <FileDetailsPanel
            v-if="detailsVisible"
            :item="detailItem"
            :commit="browseCommit"
            :selection="selectedItems"
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
            :renaming-path="renamingPath"
            @rename-commit="commitRenameEntry"
            @rename-cancel="cancelRenameEntry"
            @move="handleMoveEntry"
            @transfer="openTransferDialog([$event])"
            @delete="handleDelete"
          />
        </div>

        <div class="desktop-status-bar">
          <span>{{
            searchActive
              ? `搜索结果 ${searchResults.length} 项`
              : filesStore.hasMoreFiles
                ? `当前目录 ${files.length} / ${filesStore.totalFiles} 项`
                : `当前目录 ${files.length} 项`
          }}</span>
          <span>
            {{
              parentPath != null
                ? "单击文件夹进入，点“返回上一级”回退"
                : "单击文件夹进入子目录"
            }}
          </span>
          <span v-if="selectedCount > 0">已选 {{ selectedCount }} 项</span>
          <span v-if="typeAheadPrefix" class="desktop-status-typeahead">
            定位：<strong>{{ typeAheadPrefix }}</strong>
          </span>
          <button
            class="desktop-status-shortcuts is-hidden-touch"
            type="button"
            title="查看全部键盘快捷键"
            @click="showShortcuts = true"
          >
            <IconKeyboard :size="14" />
            <span>Ctrl/⌘+A 全选 · ↑↓ 移动 · Delete 删除 · F2 重命名</span>
            <span class="desktop-status-shortcuts-more">全部快捷键（?）</span>
          </button>
        </div>
      </template>

      <template v-else>
        <FileSkeleton
          v-if="loading"
          :variant="viewMode === 'grid' ? 'grid' : 'list'"
          :thumbnail-size="fileView.thumbnailSize"
        />

        <div v-else-if="error" class="notification is-danger is-light">
          <IconAlertCircle :size="20" class="mr-2" />
          {{ error }}
          <div class="mt-2">
            <button
              class="button is-small is-danger is-light"
              :class="{ 'is-loading': loading }"
              :disabled="loading"
              @click="refresh"
            >
              <IconRefresh :size="16" class="mr-1" />
              重试
            </button>
          </div>
        </div>

        <EmptyState
          v-else-if="!searchActive && files.length === 0"
          :icon="IconFolderOpen"
          title="此文件夹为空"
          hint="点击右下角按钮上传文件"
        />

        <div v-else-if="searchActive" class="file-list">
          <SearchResultToolbar
            :results="searchResults"
            :active="resultFilter"
            :query="searchQuery"
            :visible-count="visibleSearchResults.length"
            @update:active="setResultFilter"
          />
          <EmptyState
            v-if="searchResults.length === 0"
            :icon="IconSearch"
            compact
            title="没有找到匹配的文件"
            hint="换个关键字，或清空搜索条件"
          >
            <template #actions>
              <button class="vf-ghost-button" @click="clearSearch">
                <span>清空搜索</span>
              </button>
            </template>
          </EmptyState>
          <FileGrid
            v-if="viewMode === 'grid' && visibleSearchResults.length"
            :files="visibleSearchResults"
            :highlight="searchQuery"
            :commit="browseCommit"
            :select-mode="batchMode"
            :selected-paths="selectedPaths"
            :thumbnail-size="fileView.thumbnailSize"
            @click="handleItemClick"
            @download="handleDownload"
            @rename="handleRenameEntry"
            :renaming-path="renamingPath"
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
          />
          <FileList
            v-else-if="visibleSearchResults.length"
            :files="visibleSearchResults"
            :highlight="searchQuery"
            :select-mode="batchMode"
            :selected-paths="selectedPaths"
            :flash-path="dropFlashPath"
            :expanded-path="expandedFilePath"
            :show-action-column="!detailsVisible"
            @click="handleItemClick"
            @download="handleDownload"
            @rename="handleRenameEntry"
            :renaming-path="renamingPath"
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
          />

          <div
            v-if="isMobile && hasMore"
            ref="loadMoreSentinel"
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
        </div>

        <div v-else class="file-list">
          <FileGrid
            v-if="viewMode === 'grid'"
            :files="visibleFiles"
            :commit="browseCommit"
            :select-mode="batchMode"
            :selected-paths="selectedPaths"
            :thumbnail-size="fileView.thumbnailSize"
            @click="handleItemClick"
            @download="handleDownload"
            @rename="handleRenameEntry"
            :renaming-path="renamingPath"
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
          />
          <FileList
            v-else
            :files="visibleFiles"
            :select-mode="batchMode"
            :selected-paths="selectedPaths"
            :flash-path="dropFlashPath"
            :expanded-path="expandedFilePath"
            @click="handleItemClick"
            @download="handleDownload"
            @rename="handleRenameEntry"
            :renaming-path="renamingPath"
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
          />

          <div
            v-if="isMobile && hasMore"
            ref="loadMoreSentinel"
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
        </div>
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

    <!-- 历史记录对话框 -->
    <Modal
      :show="showHistory"
      :title="`文件历史: ${selectedFile?.name}`"
      :mobile-compact="true"
      wide
      @close="showHistory = false"
    >
      <VersionHistory v-if="selectedFile" :file-path="selectedFile.path" />
    </Modal>

    <!-- 分享对话框 -->
    <ShareDialog
      :is-active="showShareDialog"
      :file-path="selectedFile?.path || ''"
      :file="selectedFile || undefined"
      @close="showShareDialog = false"
    />

    <MoveDialog
      :is-active="showMoveDialog"
      :items="moveDialogItems"
      :initial-path="moveDialogInitialPath"
      :confirm-loading="moveDialogSubmitting"
      @close="closeMoveDialog"
      @confirm="submitMoveDialog"
    />

    <ContextMenu
      :show="contextMenu.show"
      :x="contextMenu.x"
      :y="contextMenu.y"
      :items="contextMenuItems"
      @select="handleContextMenuSelect"
      @close="contextMenu.show = false"
    />

    <!-- 预览对话框（当前版本） -->
    <Modal
      :show="showDetailsDialog"
      :title="`详细信息: ${detailsDialogFile?.name || ''}`"
      :mobile-compact="true"
      @close="showDetailsDialog = false"
    >
      <FileDetailsContent v-if="detailsDialogFile" :file="detailsDialogFile">
        <template #actions>
          <button
            v-if="detailsDialogFile.kind === 'file'"
            class="vf-ghost-button"
            @click="
              handlePreview(detailsDialogFile);
              showDetailsDialog = false;
            "
          >
            <IconEye :size="16" />
            <span>预览</span>
          </button>
          <button
            class="vf-ghost-button"
            @click="handleDownload(detailsDialogFile)"
          >
            <IconDownload :size="16" />
            <span>下载</span>
          </button>
          <button
            class="vf-ghost-button"
            @click="handleShare(detailsDialogFile)"
          >
            <IconShare :size="16" />
            <span>分享</span>
          </button>
        </template>
      </FileDetailsContent>
    </Modal>

    <UploadDropOverlay
      :visible="externalDropActive"
      :target-label="dropTargetLabel"
    />

    <TransferOwnershipDialog
      ref="transferDialogRef"
      :show="showTransferDialog"
      :items="transferItems"
      @close="closeTransferDialog"
      @transfer="submitTransfer"
    />

    <KeyboardShortcutsDialog
      :show="showShortcuts"
      @close="showShortcuts = false"
    />

    <FilePreviewModal
      :show="preview.open"
      :filename="previewFilename"
      :preview="preview"
      :can-go-prev="canGoPrev"
      :can-go-next="canGoNext"
      :position="previewIndex + 1"
      :total="previewTotal"
      :can-copy="canCopyPreview"
      :copy-state="previewCopyState"
      :file="previewFile"
      @close="closePreview"
      @retry="openPreview"
      @prev="prevPreview"
      @next="nextPreview"
      @copy="copyPreviewContent"
      @download="downloadPreviewedFile"
      @reveal="revealPreviewedFile"
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
  nextTick,
} from "vue";
import { storeToRefs } from "pinia";
import {
  IconFolderOpen,
  IconFolderPlus,
  IconAlertCircle,
  IconSearch,
  IconFileSearch,
  IconRefresh,
  IconUpload,
  IconEye,
  IconHistory,
  IconPencil,
  IconArrowsDiff,
  IconDownload,
  IconShare,
  IconUserShare,
  IconTrash,
  IconInfoCircle,
  IconKeyboard,
  IconStar,
  IconStarFilled,
} from "@tabler/icons-vue";
import { filesService } from "../../services/files.service";
import { useFilesStore } from "../../stores/files.store";
import { useAppStore } from "../../stores/app.store";
import { useAuthStore } from "../../stores/auth.store";
import { useFileViewStore } from "../../stores/fileView.store";
import FileList from "./FileList.vue";
import KeyboardShortcutsDialog from "./KeyboardShortcutsDialog.vue";
import { confirmDialog } from "../../composables/dialog";
import TransferOwnershipDialog from "./TransferOwnershipDialog.vue";
import SearchResultToolbar from "./SearchResultToolbar.vue";
import FileDetailsPanel from "./FileDetailsPanel.vue";
import BatchActionBar from "./BatchActionBar.vue";
import FilePreviewModal from "./FilePreviewModal.vue";
import UploadDropOverlay from "./UploadDropOverlay.vue";
import FileDetailsContent from "./FileDetailsContent.vue";
import DirectoryTree from "./DirectoryTree.vue";
import SidebarOverview from "./SidebarOverview.vue";
import EmptyState from "../common/EmptyState.vue";
import FileGrid from "./FileGrid.vue";
import MobileSearchBar from "./MobileSearchBar.vue";
import DesktopCommandBar from "./DesktopCommandBar.vue";
import type { SearchType } from "./BrowserSearchBox.vue";
import Breadcrumb from "./Breadcrumb.vue";
import FileSkeleton from "./FileSkeleton.vue";
import DownloadQueuePanel from "./DownloadQueuePanel.vue";
import ContextMenu, { type ContextMenuItem } from "./ContextMenu.vue";
import MoveDialog from "./MoveDialog.vue";
import FileUploader from "../file-uploader/FileUploader.vue";
import VersionHistory from "../version-history/VersionHistory.vue";
import Modal from "../common/Modal.vue";
import ShareDialog from "../common/ShareDialog.vue";
import { copyText } from "../../utils/clipboard";
import { useDownloadQueue } from "../../composables/useDownloadQueue";
import { useFilePreview } from "../../composables/useFilePreview";
import {
  useFileSearch,
  type SearchResultFilter,
} from "../../composables/useFileSearch";
import { useDirectoryManager } from "../../composables/useDirectoryManager";
import { useFileSelection } from "../../composables/useFileSelection";
import { useWindowFileDrop } from "../../composables/useWindowFileDrop";
import { columnsFromElements } from "../../utils/gridLayout";
import { fileIconKind } from "../../utils/filePresentation";
import { useMoveDialog } from "../../composables/useMoveDialog";
import { useTouchGestures } from "../../composables/useTouchGestures";
import type { FileInfo } from "../../types";
import {
  sortBrowserItems,
  sortFiles,
  type SortField,
  type SortState,
} from "../../utils/fileSort";
import { buildSiblingPath, isSafeDirName } from "../../utils/filePaths";

const filesStore = useFilesStore();
const appStore = useAppStore();
const authStore = useAuthStore();
const fileView = useFileViewStore();
const { files, loading, error, currentPath, browseCommit } =
  storeToRefs(filesStore);

const sortState = computed<SortState>(() => ({
  field: fileView.sortField,
  direction: fileView.sortDirection,
  foldersFirst: fileView.foldersFirst,
}));
const viewMode = computed(() => fileView.mode);

const searchContentEnabled = computed(
  () => authStore.features?.searchContent ?? false,
);

const isMobile = ref(false);
/** 目录树需要额外一列宽度，仅在宽屏桌面显示。 */
const isWideScreen = ref(false);

const MOBILE_LAYOUT_MEDIA_QUERY = "(max-width: 1023px)";
const WIDE_LAYOUT_MEDIA_QUERY = "(min-width: 1280px)";

function updateIsMobile() {
  isMobile.value = window.matchMedia(MOBILE_LAYOUT_MEDIA_QUERY).matches;
}

function updateIsWideScreen() {
  isWideScreen.value = window.matchMedia(WIDE_LAYOUT_MEDIA_QUERY).matches;
}

const showUploader = ref(false);
/**
 * 打开上传对话框时直接弹出的选择器：
 * 工具栏「上传」主按钮 → 文件；「上传文件夹」菜单项 → 目录；其它入口不自动弹出。
 */
const uploaderPick = ref<"files" | "directory" | null>(null);

function openUploader(pick: "files" | "directory" | null = null) {
  uploaderPick.value = pick;
  showUploader.value = true;
}

function closeUploader() {
  showUploader.value = false;
  uploaderPick.value = null;
}
const showDetailsDialog = ref(false);
/** 正在内联重命名的条目路径（空字符串表示没有）。 */
const renamingPath = ref("");
/** 工具栏上的上传进度：有队列时显示「上传中 x/y」，点击打开上传对话框。 */
const uploadIndicator = computed(() => {
  const summary = fileUploaderRef.value?.summary;
  if (!summary || summary.total === 0) return null;

  const finished = summary.done + summary.failed;
  const label =
    summary.active > 0
      ? `上传中 ${finished}/${summary.total}`
      : summary.queued > 0
        ? `待上传 ${summary.total}`
        : `上传完成 ${summary.done}/${summary.total}`;
  const title =
    summary.failed > 0
      ? `上传队列：${summary.done} 成功，${summary.failed} 失败`
      : "查看上传队列";

  return { label, title };
});

/** 文件列表重新加载或收藏变化后递增，用于让侧栏概览刷新。 */
const sidebarVersion = ref(0);
/** 已收藏的条目路径，用于右键菜单里的星标状态。 */
const favoritePaths = ref<Set<string>>(new Set());
const detailsDialogFile = ref<FileInfo | null>(null);
const showHistory = ref(false);
const showShareDialog = ref(false);
const showShortcuts = ref(false);
const showTransferDialog = ref(false);
const transferItems = ref<FileInfo[]>([]);
const transferDialogRef = ref<{ finish: () => void } | null>(null);

/** 打开所有权转移对话框（支持单条与批量）。 */
function openTransferDialog(items: FileInfo[]) {
  const targets = items.filter((item) => item.path !== "");
  if (targets.length === 0) return;
  transferItems.value = targets;
  showTransferDialog.value = true;
}

function closeTransferDialog() {
  showTransferDialog.value = false;
  transferItems.value = [];
}

/** 提交转移：成功则刷新列表并清空选择。 */
async function submitTransfer(
  target: { id: string; username: string },
  message: string,
) {
  const paths = transferItems.value.map((item) => item.path);
  try {
    const result = await filesService.transferOwnership(
      paths,
      target.id,
      message || undefined,
    );
    appStore.success(
      `已把 ${result.transferred} 个条目转移给 ${result.target_username}`,
    );
    closeTransferDialog();
    clearSelection();
    await refresh();
  } catch (e) {
    appStore.error(e instanceof Error ? e.message : "转移失败");
    transferDialogRef.value?.finish();
  }
}
const selectedFile = ref<FileInfo | null>(null);
const fileUploaderRef = ref<InstanceType<typeof FileUploader> | null>(null);
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

/** 左侧目录树：宽屏桌面显示。 */
const treeVisible = computed(() => !isMobile.value && isWideScreen.value);

function handleFavoritesChanged(entries: { path: string }[]) {
  const next = new Set(entries.map((entry) => entry.path));
  // 侧栏自身加载后也会上报；只有集合真的变化时才让侧栏重新拉取，
  // 否则「加载 → 上报 → 再加载」会形成请求死循环
  const changed =
    next.size !== favoritePaths.value.size ||
    [...next].some((path) => !favoritePaths.value.has(path));
  favoritePaths.value = next;

  if (changed) {
    sidebarVersion.value += 1;
  }
}

/** 点击侧栏「收藏」：跳到该条目所在目录（文件则同时设为活动行）。 */
function handleOpenFavorite(entry: { path: string; kind: string }) {
  if (searchActive.value) clearSearch();
  if (entry.kind === "directory") {
    filesStore.navigateTo(entry.path);
    return;
  }
  handleOpenRecentFile(entry);
}

/** 切换收藏状态（右键菜单与侧栏共用）。 */
async function toggleFavorite(file: FileInfo) {
  const isFavorite = favoritePaths.value.has(file.path);
  try {
    const entries = isFavorite
      ? await filesService.removeFavorite(file.path)
      : await filesService.addFavorite(file.path);
    handleFavoritesChanged(entries);
    appStore.success(
      isFavorite ? `已取消收藏「${file.name}」` : `已收藏「${file.name}」`,
    );
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "收藏操作失败");
  }
}

/** 点击侧栏「最近更新」：跳到文件所在目录并把它设为活动行。 */
function handleOpenRecentFile(file: { path: string }) {
  if (searchActive.value) clearSearch();
  const parent = file.path.includes("/")
    ? file.path.slice(0, file.path.lastIndexOf("/"))
    : "";
  filesStore.navigateTo(parent);
  desktopActivePath.value = file.path;
}

function handleTreeNavigate(path: string) {
  if (searchActive.value) clearSearch();
  filesStore.navigateTo(path);
}

/**
 * 右侧「详细信息」面板：桌面端显示，移动端隐藏。
 * 展示当前活动条目（键盘高亮或唯一选中项，见 findActiveItem）。
 */
const detailsVisible = computed(
  () => !isMobile.value && fileView.detailsVisible,
);

/** 移动端搜索筛选面板（默认收起，保持顶部只有一行搜索框）。 */
const mobileSearchFiltersOpen = ref(false);
const detailItem = computed<FileInfo | undefined>(() => findActiveItem());

/** 当前批量选中的条目（保持列表顺序），用于详情面板的多选摘要。 */
const selectedItems = computed<FileInfo[]>(() =>
  activeList.value.filter((file) => selectedPaths.value.has(file.path)),
);

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

function queueDroppedFiles(files: File[]) {
  showUploader.value = true;
  void nextTick().then(() => {
    fileUploaderRef.value?.addFiles(files);
    appStore.success(`已添加 ${files.length} 个文件到上传队列`);
  });
}

const { dragging: externalDropActive } = useWindowFileDrop({
  onFiles: queueDroppedFiles,
  enabled: () => !anyDialogOpen.value,
});

/** 代码/文本预览支持一键复制原文，复制结果在按钮上就地反馈。 */
const previewCopyState = ref<"idle" | "done" | "failed">("idle");
const canCopyPreview = computed(
  () =>
    (preview.value.kind === "code" || preview.value.kind === "text") &&
    !preview.value.loading &&
    !preview.value.error &&
    preview.value.text.length > 0,
);

/** 当前预览的条目：用于顶栏图标、大小与「下载 / 所在文件夹」入口。 */
const previewFile = computed<FileInfo | null>(() => {
  const path = preview.value.path;
  if (!path) return null;
  return (
    [...files.value, ...searchResults.value].find(
      (item) => item.path === path,
    ) ?? null
  );
});

function downloadPreviewedFile() {
  const file = previewFile.value;
  if (file) handleDownload(file);
}

/** 打开所在文件夹并高亮该文件（主流预览器的「在文件夹中显示」）。 */
async function revealPreviewedFile() {
  const file = previewFile.value;
  if (!file) return;
  const slash = file.path.lastIndexOf("/");
  const parent = slash > 0 ? file.path.slice(0, slash) : "";

  closePreview();
  if (searchActive.value) clearSearch();
  await filesStore.loadFiles(parent);
  desktopActivePath.value = file.path;
}

async function copyPreviewContent() {
  const ok = await copyText(preview.value.text);
  previewCopyState.value = ok ? "done" : "failed";
  window.setTimeout(() => {
    previewCopyState.value = "idle";
  }, 2000);
}

watch(
  () => preview.value.path,
  () => {
    previewCopyState.value = "idle";
  },
);

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
  renameEntry: handleRenameEntry,
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

const contextMenu = ref<{
  show: boolean;
  x: number;
  y: number;
  file: FileInfo | null;
}>({ show: false, x: 0, y: 0, file: null });

const contextMenuItems = computed<ContextMenuItem[]>(() => {
  const file = contextMenu.value.file;
  if (!file) return [];
  const isDirectory = file.kind === "directory";

  const items: ContextMenuItem[] = [];
  if (isDirectory) {
    items.push({ key: "open", label: "打开", icon: IconFolderOpen });
    items.push({
      key: "create-directory",
      label: "在此新建子目录",
      icon: IconFolderPlus,
    });
  } else {
    items.push({ key: "preview", label: "预览", icon: IconEye });
    items.push({ key: "history", label: "历史版本", icon: IconHistory });
  }
  const isFavorite = favoritePaths.value.has(file.path);
  items.push({ key: "details", label: "详细信息", icon: IconInfoCircle });
  items.push({
    key: "favorite",
    label: isFavorite ? "取消收藏" : "加入收藏",
    icon: isFavorite ? IconStarFilled : IconStar,
  });
  items.push({ key: "rename", label: "重命名", icon: IconPencil });
  items.push({ key: "move", label: "移动", icon: IconArrowsDiff });
  items.push({
    key: "transfer",
    label: "转移所有权",
    icon: IconUserShare,
  });
  items.push({ key: "download", label: "下载", icon: IconDownload });
  items.push({ key: "share", label: "分享", icon: IconShare });
  items.push({ key: "delete", label: "删除", icon: IconTrash, danger: true });
  return items;
});

onMounted(() => {
  filesStore.loadFiles();

  const onDocPointer = (e: MouseEvent | TouchEvent) => {
    const target = e.target as Node | null;
    if (!target) return;

    if (desktopSearchOpen.value) {
      const searchBox = desktopSearchBoxRef.value;
      if (searchBox && !searchBox.contains(target)) closeDesktopSearch();
    }
  };

  const onDocKeydown = (e: KeyboardEvent) => {
    if (e.defaultPrevented) return;

    // Escape 逐层退出：按键定位前缀 → 高级搜索 → 预览 → 批量模式 → 选择
    if (e.key === "Escape") {
      if (typeAheadPrefix.value) {
        e.preventDefault();
        typeAheadPrefix.value = "";
        if (typeAheadTimer) window.clearTimeout(typeAheadTimer);
        return;
      }
      if (desktopSearchOpen.value) {
        closeDesktopSearch();
        return;
      }
      if (preview.value.open) {
        closePreview();
        return;
      }
      if (batchMode.value) {
        toggleBatchMode();
        return;
      }
      if (selectedPaths.value.size > 0) clearSelection();
      return;
    }

    // 预览打开时用左右方向键切换上一个/下一个
    if (preview.value.open) {
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        prevPreview();
        return;
      }
      if (e.key === "ArrowRight") {
        e.preventDefault();
        nextPreview();
        return;
      }
    }

    // ? 打开/关闭快捷键面板：打开时再按一次即可关闭（此时焦点可能在面板内）
    if (e.key === "?") {
      if (showShortcuts.value) {
        e.preventDefault();
        showShortcuts.value = false;
        return;
      }
      if (!isTypingTarget(e.target)) {
        e.preventDefault();
        showShortcuts.value = true;
        return;
      }
    }

    // 输入控件或弹窗内不触发文件操作快捷键
    if (anyOverlayOpen() || isTypingTarget(e.target)) return;

    const modifier = e.ctrlKey || e.metaKey;
    if (modifier && (e.key === "a" || e.key === "A")) {
      e.preventDefault();
      if (!batchMode.value) batchMode.value = true;
      selectAllVisible();
      return;
    }

    if (e.key === "Delete" || e.key === "Backspace") {
      if (selectedPaths.value.size > 0) {
        e.preventDefault();
        void batchDelete();
        return;
      }
      const active = findActiveItem();
      if (active) {
        e.preventDefault();
        void handleDelete(active);
      }
      return;
    }

    if (
      e.key === "ArrowUp" ||
      e.key === "ArrowDown" ||
      e.key === "ArrowLeft" ||
      e.key === "ArrowRight" ||
      e.key === "Home" ||
      e.key === "End" ||
      e.key === "PageUp" ||
      e.key === "PageDown"
    ) {
      e.preventDefault();
      moveActiveRow(e.key, e.shiftKey);
      return;
    }

    // Space：切换活动行选中态
    if (e.key === " " || e.key === "Spacebar") {
      e.preventDefault();
      toggleActiveRowSelection();
      return;
    }

    // 可打印字符（不含组合键）：按名称前缀定位；有匹配才拦截，否则放行
    if (
      !modifier &&
      !e.altKey &&
      e.key.length === 1 &&
      e.key !== " " &&
      /[\p{L}\p{N}._-]/u.test(e.key) &&
      handleTypeAhead(e.key)
    ) {
      e.preventDefault();
      return;
    }

    // 键盘打开行菜单（Windows 惯例：Shift+F10 或菜单键）
    if ((e.shiftKey && e.key === "F10") || e.key === "ContextMenu") {
      const target = findActiveItem();
      if (target) {
        e.preventDefault();
        openRowContextMenu(target);
        return;
      }
    }

    if (e.key === "F2") {
      if (selectedPaths.value.size === 1) {
        e.preventDefault();
        void renameSelected();
        return;
      }
      const active = findActiveItem();
      if (active) {
        e.preventDefault();
        void handleRenameEntry(active);
      }
      return;
    }

    if (e.key === "Enter") {
      const active = findActiveItem();
      if (!active) return;
      e.preventDefault();
      if (active.kind === "directory") {
        handleOpenFolder(active);
      } else {
        handlePreview(active);
      }
    }
  };

  document.addEventListener("click", onDocPointer, true);
  document.addEventListener("touchstart", onDocPointer, true);
  document.addEventListener("keydown", onDocKeydown);
  onBeforeUnmount(() => {
    document.removeEventListener("click", onDocPointer, true);
    document.removeEventListener("touchstart", onDocPointer, true);
    document.removeEventListener("keydown", onDocKeydown);
  });

  updateIsMobile();
  updateIsWideScreen();
  const watchMedia = (query: string, handler: () => void): void => {
    try {
      const mql = window.matchMedia(query);
      if ("addEventListener" in mql) {
        mql.addEventListener("change", handler);
        onBeforeUnmount(() => mql.removeEventListener("change", handler));
      } else {
        // @ts-expect-error older Safari
        mql.addListener(handler);
        // @ts-expect-error older Safari
        onBeforeUnmount(() => mql.removeListener(handler));
      }
    } catch {
      // matchMedia 不可用时按默认值处理
    }
  };

  watchMedia(MOBILE_LAYOUT_MEDIA_QUERY, updateIsMobile);
  watchMedia(WIDE_LAYOUT_MEDIA_QUERY, updateIsWideScreen);
});

onBeforeUnmount(() => {
  closePreview();
});

function navigateTo(path: string) {
  expandedFilePath.value = "";
  filesStore.navigateTo(path);
}

/** 面包屑跳转：处于搜索结果时先退出搜索，再进入目标目录。 */
function handleBreadcrumbNavigate(path: string) {
  if (searchActive.value) clearSearch();
  navigateTo(path);
}

function refresh() {
  expandedFilePath.value = "";
  return filesStore.loadFiles(filesStore.currentPath);
}

function goBack() {
  expandedFilePath.value = "";
  filesStore.goBack();
}

function goRoot() {
  navigateTo("");
}

const parentPath = computed<string | null>(() => {
  const cur = currentPath.value || "";
  const parts = cur.split("/").filter(Boolean);
  if (parts.length === 0) return null;
  parts.pop();
  return parts.join("/");
});

function normalizeEntryName(
  rawName: string,
  invalidMessage: string,
): string | null {
  const name = rawName.trim();
  if (!isSafeDirName(name)) {
    appStore.error(invalidMessage);
    return null;
  }
  return name;
}

// 4.2: 大数据目录的分批渲染（桌面与移动端通用）
const INITIAL_VISIBLE_COUNT = 40;
const VISIBLE_CHUNK_COUNT = 30;
const visibleCount = ref(INITIAL_VISIBLE_COUNT);
const loadMoreSentinel = ref<HTMLElement | null>(null);
let loadMoreObserver: IntersectionObserver | null = null;

/** 目录列表（不含 `.`/`..` 之类的合成条目，导航交给面包屑/目录树/上一级按钮）。 */
const navigationListItems = computed<FileInfo[]>(() =>
  sortBrowserItems(files.value, sortState.value),
);

/** 搜索结果按图标分类归入筛选桶（与搜索工具条的 chips 口径一致）。 */
function searchResultBucket(file: FileInfo): SearchResultFilter {
  switch (fileIconKind(file)) {
    case "folder":
      return "folder";
    case "image":
      return "image";
    case "video":
      return "video";
    case "audio":
      return "audio";
    case "text":
    case "code":
    case "pdf":
      return "document";
    default:
      return "other";
  }
}

const filteredSearchResults = computed<FileInfo[]>(() => {
  if (resultFilter.value === "all") return searchResults.value;
  return searchResults.value.filter(
    (file) => searchResultBucket(file) === resultFilter.value,
  );
});

const sortedSearchResults = computed<FileInfo[]>(() => {
  return sortFiles(filteredSearchResults.value, sortState.value);
});

const activeList = computed(() =>
  searchActive.value ? sortedSearchResults.value : navigationListItems.value,
);
// 本地分批渲染之外，目录可能还有服务端未加载的页（服务端分页，默认每页 200 条）。
const hasMoreLocal = computed(
  () => visibleCount.value < activeList.value.length,
);
const hasMore = computed(() => {
  if (searchActive.value) return hasMoreLocal.value || searchHasMore.value;
  return hasMoreLocal.value || filesStore.hasMoreFiles;
});
// 当前目录的已知条目总数（含 "." 与 ".." 两个虚拟条目），用于进度提示。
const knownTotal = computed(() =>
  searchActive.value ? activeList.value.length : filesStore.totalFiles + 2,
);
const visibleFiles = computed(() =>
  navigationListItems.value.slice(0, visibleCount.value),
);
const visibleSearchResults = computed(() =>
  sortedSearchResults.value.slice(0, visibleCount.value),
);
const desktopItems = computed(() =>
  (searchActive.value
    ? sortedSearchResults.value
    : navigationListItems.value
  ).slice(0, visibleCount.value),
);
const desktopActivePath = ref("");

// 文件列表重新加载（上传/删除/重命名/移动/切换目录）后刷新侧栏概览
// 移动端底栏的「新建文件夹」：在当前位置打开创建对话框
watch(
  () => appStore.createDirectoryRequests,
  () => {
    void createDirectoryHere();
  },
);

watch(
  () => filesStore.files,
  () => {
    sidebarVersion.value += 1;
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

function bumpVisibleCount() {
  const total = activeList.value.length;
  visibleCount.value = Math.min(
    total,
    visibleCount.value + VISIBLE_CHUNK_COUNT,
  );
}

function resetVisibleCount() {
  visibleCount.value = INITIAL_VISIBLE_COUNT;
}

/**
 * 分批渲染：只要哨兵还在视口附近就继续补齐，避免 IntersectionObserver
 * 在同一交叉状态下不再回调导致列表停在首批。
 */
function maybeLoadMore() {
  if (!hasMore.value) return;
  const sentinel = loadMoreSentinel.value;
  if (!sentinel) return;

  const rect = sentinel.getBoundingClientRect();
  const viewportHeight =
    window.innerHeight || document.documentElement.clientHeight || 0;
  if (rect.top > viewportHeight + 160) return;

  // 先补齐本地已加载条目的渲染分片。
  if (hasMoreLocal.value) {
    const before = visibleCount.value;
    bumpVisibleCount();
    if (visibleCount.value === before) return;
    void nextTick().then(() => maybeLoadMore());
    return;
  }

  // 本地已全部渲染但服务端还有下一页：按需拉取，成功后由 length 监听继续补齐。
  if (searchActive.value) {
    if (searchHasMore.value) void loadMoreSearchResults();
    return;
  }

  if (filesStore.hasMoreFiles) {
    void filesStore.loadMoreFiles();
  }
}

function setupLoadMoreObserver() {
  if (loadMoreObserver) {
    loadMoreObserver.disconnect();
    loadMoreObserver = null;
  }

  if (!("IntersectionObserver" in window)) return;
  if (!loadMoreSentinel.value) return;

  loadMoreObserver = new IntersectionObserver(
    (entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      maybeLoadMore();
    },
    { root: null, threshold: 0.1, rootMargin: "120px" },
  );

  loadMoreObserver.observe(loadMoreSentinel.value);
}

watch(
  [
    () => filesStore.currentPath,
    searchActive,
    searchQuery,
    () => searchResults.value.length,
  ],
  () => {
    resetVisibleCount();
    void nextTick().then(() => setupLoadMoreObserver());
  },
);

watch([isMobile, loadMoreSentinel], () => {
  void nextTick().then(() => setupLoadMoreObserver());
});

// 服务端分页追加数据后，若哨兵仍在视口内则继续拉取下一页。
watch(
  () => files.value.length,
  () => {
    void nextTick().then(() => maybeLoadMore());
  },
);

onBeforeUnmount(() => {
  if (loadMoreObserver) {
    loadMoreObserver.disconnect();
    loadMoreObserver = null;
  }
});

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

async function handleDelete(file: FileInfo) {
  // 删除不可恢复（版本历史随条目级联删除），必须二次确认
  const isDirectory = file.kind === "directory";
  const ok = await confirmDialog({
    title: isDirectory ? "删除目录" : "删除文件",
    message: isDirectory
      ? `确定要删除“${file.name}”吗？目录及其全部内容将被删除，且不可撤销。`
      : `确定要删除“${file.name}”吗？删除后不可撤销。`,
    confirmText: "删除",
    danger: true,
  });
  if (!ok) return;

  try {
    await filesStore.deleteFile(
      file.path,
      `${file.kind === "directory" ? "删除目录" : "删除文件"}: ${file.path}`,
    );
    if (desktopActivePath.value === file.path) {
      desktopActivePath.value = "";
    }
    if (searchActive.value) {
      await doSearch(false);
    }
    appStore.success(`已删除「${file.name}」`);
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "删除失败");
  }
}

async function handleCreateDirectory(file: FileInfo) {
  if (file.kind !== "directory") return;
  await promptCreateDirectory(file.path);
}

/** 在当前目录下新建文件夹（工具栏按钮 / 移动端入口）。 */
async function createDirectoryHere() {
  await promptCreateDirectory(currentPath.value || "");
}

/** 打开内联重命名（F2 / 右键菜单 / 行内按钮）；同时把该行设为活动行。 */
function handleRenameEntry(file: FileInfo) {
  desktopActivePath.value = file.path;
  renamingPath.value = file.path;
}

/** 提交内联重命名：校验名称后调用重命名接口。 */
async function commitRenameEntry(file: FileInfo, rawName: string) {
  renamingPath.value = "";
  if (!rawName) return;

  const name = normalizeEntryName(rawName, "非法名称");
  if (!name) return;
  if (name === file.name) {
    appStore.error("名称未变化");
    return;
  }

  try {
    const to = await renameEntryPath(
      file.path,
      name,
      `重命名${file.kind === "directory" ? "目录" : "项目"}: ${file.path} -> ${buildSiblingPath(file.path, name)}`,
    );
    replaceSelectedPath(file.path, to);
    if (desktopActivePath.value === file.path) {
      desktopActivePath.value = to;
    }
    appStore.success(
      `已重命名为「${to.split("/").filter(Boolean).pop() ?? to}」`,
    );
    await refreshAfterMutation();
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "重命名失败");
  }
}

function cancelRenameEntry() {
  renamingPath.value = "";
}

function handleMoveEntry(file: FileInfo) {
  openMoveForEntry(file);
}

/** 当前正在拖动的条目（用于拖放移动）。 */
const draggingFile = ref<FileInfo | null>(null);

function handleDragStart(file: FileInfo) {
  draggingFile.value = file;
}

function handleDragEnd() {
  draggingFile.value = null;
}

/** 光标跟随拖拽 chip：位置由捕获阶段 dragover 实时驱动（定位更新非动画 ✓ 无需降级）。 */
const dragChipPos = ref({ x: -999, y: -999 });
const dragChipTarget = ref<string | null>(null);
const dragChipInvalid = ref(false);
const dragChipLabel = computed(() => {
  const file = draggingFile.value;
  if (!file) return "";
  if (dragChipInvalid.value) return "不能放到这里";
  if (dragChipTarget.value) return `放入 ${dragChipTarget.value}`;
  const sel = selectedPaths.value;
  if (sel instanceof Set && sel.size > 1 && sel.has(file.path)) {
    return `移动 ${sel.size} 项`;
  }
  return `移动 ${file.name}`;
});
function moveDragChip(e: DragEvent) {
  dragChipPos.value = { x: e.clientX + 14, y: e.clientY + 14 };
  // 两段式：命中有效落点（树条目 / 目录行——目录行带名称链接 ✓）→「放入 目标名」
  const el = document.elementFromPoint(e.clientX, e.clientY);
  const tree = el?.closest(".directory-tree-item") as HTMLElement | null;
  const row = el?.closest("tr[data-vfiles-path]") as HTMLElement | null;
  const rowIsDir = row?.querySelector("a.desktop-name-link") != null;
  const target = tree ?? (rowIsDir ? row : null);
  const targetPath =
    target?.getAttribute("data-vfiles-path") ??
    row?.getAttribute("data-vfiles-path") ??
    "";
  const draggedPath = draggingFile.value?.path ?? "";
  // 拖自身任何行 = 禁止（先于目录目标判定）
  if (
    row &&
    draggedPath &&
    row.getAttribute("data-vfiles-path") === draggedPath
  ) {
    dragChipTarget.value = null;
    dragChipInvalid.value = true;
    return;
  }
  // 非法目标（拖自身 / 自身子目录）→ 禁止态（useMoveDialog 落子校验的前置预示）
  if (
    target &&
    draggedPath &&
    (targetPath === draggedPath || targetPath.startsWith(`${draggedPath}/`))
  ) {
    dragChipTarget.value = null;
    dragChipInvalid.value = true;
    return;
  }
  dragChipInvalid.value = false;
  if (!target) {
    dragChipTarget.value = null;
    return;
  }
  dragChipTarget.value =
    (
      target.querySelector(".desktop-name-text, [class*=tree-name]")
        ?.textContent ??
      target.textContent ??
      ""
    )
      .trim()
      .split("\n")[0]
      .slice(0, 24) || null;
}
watch(draggingFile, (cur) => {
  if (cur) {
    document.addEventListener("dragover", moveDragChip, {
      passive: true,
      capture: true,
    });
  } else {
    document.removeEventListener("dragover", moveDragChip, { capture: true });
    dragChipPos.value = { x: -999, y: -999 };
    dragChipTarget.value = null;
    dragChipInvalid.value = false;
  }
});
onBeforeUnmount(() => {
  document.removeEventListener("dragover", moveDragChip, { capture: true });
});

const dropFlashPath = ref("");

/** 搜索框 ↓ 键：进入结果列表首项（活动项模型，容器级 ↑↓ 随后自然接管 ✓） */
function enterSearchResults() {
  const first = searchResults.value[0];
  if (!first) return;
  desktopActivePath.value = first.path;
}

async function handleDropOnFolder(targetDir: string) {
  const file = draggingFile.value;
  draggingFile.value = null;
  if (!file) return;
  // 落子回执（Finder/Explorer 式）：目标行 650ms 高亮消退（父层持态 ✓ 跨 remount）
  dropFlashPath.value = targetDir;
  window.setTimeout(() => {
    if (dropFlashPath.value === targetDir) dropFlashPath.value = "";
  }, 650);
  // 校验（自身/子目录、未变化、重名）由 useMoveDialog 内的 planMoveOperations 统一处理
  await moveEntryToDirectory(file, targetDir);
}

function handleViewHistory(file: FileInfo) {
  selectedFile.value = file;
  showHistory.value = true;
}

function handleShare(file: FileInfo) {
  selectedFile.value = file;
  showShareDialog.value = true;
}

async function handleUpload() {
  showUploader.value = false;
  appStore.success("文件上传成功");
  await refresh();
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

const OVERLAY_INPUT_TAGS = new Set(["INPUT", "TEXTAREA", "SELECT"]);

function isTypingTarget(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element) return false;
  if (OVERLAY_INPUT_TAGS.has(element.tagName)) return true;
  return element.isContentEditable === true;
}

function anyOverlayOpen(): boolean {
  return (
    preview.value.open ||
    showShortcuts.value ||
    showUploader.value ||
    showHistory.value ||
    showShareDialog.value ||
    showMoveDialog.value ||
    dirManagerOpen.value
  );
}

/** 当前键盘操作的目标：优先高亮行，其次唯一的已选条目。 */
/**
 * 行内复选框：即使不在「批量选择」模式，勾选一项也应立刻出现操作条
 * （与主流网盘一致：勾选即进入选择态，不必先点工具栏按钮）。
 */
function handleRowSelect(file: FileInfo) {
  if (!batchMode.value) batchMode.value = true;
  toggleSelect(file);
}

function findActiveItem(): FileInfo | undefined {
  const list = searchActive.value
    ? sortedSearchResults.value
    : navigationListItems.value;

  if (desktopActivePath.value) {
    const active = list.find((file) => file.path === desktopActivePath.value);
    if (active) return active;
  }

  if (selectedPaths.value.size === 1) {
    const [only] = selectedPaths.value;
    const selected = list.find((file) => file.path === only);
    if (selected) return selected;
  }

  return undefined;
}

/**
 * 网格视图下「上下键移动一行」的步长：按实际渲染的卡片推断列数。
 *
 * jsdom 等无布局环境下 offsetTop 全为 0，会退化成「一行一张卡」，
 * 因此这里只做兜底，真正的行为由 `columnsFromElements` 决定。
 */
function gridColumnStep(): number {
  if (typeof document === "undefined") return 1;
  const cards = Array.from(document.querySelectorAll(".file-grid .file-card"));
  return cards.length > 0 ? columnsFromElements(cards) : 1;
}

/**
 * 方向键 / Home / End 移动活动行（主流文件管理器的基本键盘操作）。
 *
 * - 列表视图：上下左右都按一项移动；网格视图：左右按一项、上下按**一行**（列数由布局推断）；
 * - 普通方向键：移动高亮行，批量模式下选择也跟随移动；
 * - Shift + 方向键：从上次选中项扩展到当前行（复用 useFileSelection 的区间选择）；
 * - `.`/`..` 快捷项不参与移动与选择（与鼠标点击一致）。
 */
function moveActiveRow(key: string, shift: boolean) {
  const list = searchActive.value
    ? sortedSearchResults.value
    : navigationListItems.value;
  if (list.length === 0) return;

  const currentIndex = list.findIndex(
    (item) => item.path === desktopActivePath.value,
  );
  const rowStep = viewMode.value === "grid" ? gridColumnStep() : 1;
  let nextIndex: number;
  switch (key) {
    case "ArrowUp":
      nextIndex = currentIndex <= 0 ? 0 : Math.max(0, currentIndex - rowStep);
      break;
    case "ArrowDown":
      nextIndex =
        currentIndex === -1
          ? 0
          : Math.min(list.length - 1, currentIndex + rowStep);
      break;
    case "ArrowLeft":
      nextIndex = currentIndex <= 0 ? 0 : currentIndex - 1;
      break;
    case "ArrowRight":
      nextIndex =
        currentIndex === -1 ? 0 : Math.min(list.length - 1, currentIndex + 1);
      break;
    case "Home":
      nextIndex = 0;
      break;
    case "End":
      nextIndex = list.length - 1;
      break;
    case "PageUp":
      nextIndex = Math.max(
        0,
        (currentIndex === -1 ? 0 : currentIndex) - pageStep(),
      );
      break;
    case "PageDown":
      nextIndex =
        currentIndex === -1
          ? 0
          : Math.min(list.length - 1, currentIndex + pageStep());
      break;
    default:
      return;
  }

  const next = list[nextIndex];
  if (!next) return;

  const previousPath = desktopActivePath.value;
  const previousIsReal = list.some((item) => item.path === previousPath);

  desktopActivePath.value = next.path;
  if (shift) {
    // 还没有锚点时，把移动前的活动行作为区间起点，保证第一次 Shift+方向键
    // 就能选中「原位置 → 新位置」的区间
    if (!lastSelectedPath.value && previousIsReal) {
      lastSelectedPath.value = previousPath;
    }
    handleModifierSelect({ file: next, shift: true, meta: false });
  } else if (batchMode.value) {
    selectedPaths.value = new Set([next.path]);
  }

  // 目标行可能还没渲染（分批渲染），先补齐渲染范围再滚动到可见区域
  if (nextIndex >= visibleCount.value) {
    visibleCount.value = Math.min(activeList.value.length, nextIndex + 1);
  }
  void nextTick().then(() => scrollActiveIntoView(next.path));
}

/**
 * 一屏能移动多少个条目：列表按「可视行数」，网格按「可视行数 × 列数」。
 *
 * 依赖真实布局（行/卡片高度与容器高度），无布局环境（如 jsdom）兜底 10。
 */
function pageStep(): number {
  if (typeof document === "undefined") return 10;
  const shell = document.querySelector<HTMLElement>(".desktop-list-shell");
  if (!shell) return 10;

  if (viewMode.value === "grid") {
    const cards = Array.from(
      document.querySelectorAll<HTMLElement>(".file-grid .file-card"),
    );
    if (cards.length === 0) return 10;
    const columns = columnsFromElements(cards);
    // 行高优先取同列相邻两行的间距；只有一行时退回卡片高度 + 间距
    const cardTop = cards[0].getBoundingClientRect().top;
    const nextRow = cards[columns];
    const rowHeight = nextRow
      ? nextRow.getBoundingClientRect().top - cardTop
      : cards[0].getBoundingClientRect().height + 12;
    if (rowHeight <= 0) return 10;
    const visibleRows = Math.max(1, Math.floor(shell.clientHeight / rowHeight));
    return Math.max(1, visibleRows * columns);
  }

  const rows = Array.from(
    document.querySelectorAll<HTMLElement>("tr.desktop-file-row, .file-item"),
  );
  if (rows.length < 2) return 10;
  const rowHeight =
    rows[1].getBoundingClientRect().top - rows[0].getBoundingClientRect().top;
  if (rowHeight <= 0) return 10;
  return Math.max(1, Math.floor(shell.clientHeight / rowHeight));
}

/** 按键定位（type-ahead）：连续输入会在 800ms 后重置，主流文件管理器都有此行为。 */
const typeAheadPrefix = ref("");
let typeAheadTimer: number | undefined;

function handleTypeAhead(character: string): boolean {
  const list = searchActive.value
    ? sortedSearchResults.value
    : navigationListItems.value;
  if (list.length === 0) return false;

  typeAheadPrefix.value += character;
  if (typeAheadTimer) window.clearTimeout(typeAheadTimer);
  typeAheadTimer = window.setTimeout(() => {
    typeAheadPrefix.value = "";
  }, 800);

  const prefix = typeAheadPrefix.value.toLowerCase();
  const startIndex = Math.max(
    0,
    list.findIndex((item) => item.path === desktopActivePath.value),
  );
  // 先找当前位置之后的匹配项，绕回开头再找一次（与主流行为一致）
  const ordered = [...list.slice(startIndex), ...list.slice(0, startIndex)];
  const match = ordered.find((item) =>
    item.name.toLowerCase().startsWith(prefix),
  );
  if (!match) {
    // 无匹配：清空前缀，不显示提示，也不拦截按键
    typeAheadPrefix.value = "";
    if (typeAheadTimer) window.clearTimeout(typeAheadTimer);
    return false;
  }

  const index = list.findIndex((item) => item.path === match.path);
  desktopActivePath.value = match.path;
  if (batchMode.value) selectedPaths.value = new Set([match.path]);
  if (index >= visibleCount.value) {
    visibleCount.value = Math.min(activeList.value.length, index + 1);
  }
  void nextTick().then(() => scrollActiveIntoView(match.path));
  return true;
}

/** Space：切换活动行的选中态（主流文件列表都支持）。 */
function toggleActiveRowSelection() {
  const active = findActiveItem();
  if (!active) return;
  if (!batchMode.value) batchMode.value = true;
  toggleSelect(active);
}

function scrollActiveIntoView(path: string) {
  if (typeof document === "undefined") return;
  const selector = `[data-vfiles-path="${CSS.escape(path)}"]`;
  const element = document.querySelector<HTMLElement>(selector);
  element?.scrollIntoView?.({ block: "nearest" });
}

/** 在指定行的右侧打开操作菜单（键盘触发时按行位置定位）。 */
function openRowContextMenu(file: FileInfo) {
  if (typeof document === "undefined") return;
  const selector = `[data-vfiles-path="${CSS.escape(file.path)}"]`;
  const element = document.querySelector<HTMLElement>(selector);
  const rect = element?.getBoundingClientRect();
  const x = rect ? rect.right - 8 : 0;
  const y = rect ? rect.bottom : 0;
  handleContextMenu({ file, x, y });
}

/** 内容搜索无结果时，一键改用文件名搜索（保留关键字与范围）。 */
function switchToNameSearch() {
  searchContent.value = false;
  void runDesktopSearch();
}

function handleContextMenu(payload: { file: FileInfo; x: number; y: number }) {
  const file = payload.file;
  narrowSelectionTo(file.path);
  desktopActivePath.value = file.path;
  contextMenu.value = {
    show: true,
    x: payload.x,
    y: payload.y,
    file: payload.file,
  };
}

function openDetailsDialog(file: FileInfo) {
  detailsDialogFile.value = file;
  showDetailsDialog.value = true;
}

function handleContextMenuSelect(key: string) {
  const file = contextMenu.value.file;
  if (!file) return;

  switch (key) {
    case "details":
      openDetailsDialog(file);
      break;
    case "favorite":
      void toggleFavorite(file);
      break;
    case "open":
      handleOpenFolder(file);
      break;
    case "create-directory":
      void handleCreateDirectory(file);
      break;
    case "preview":
      handlePreview(file);
      break;
    case "history":
      handleViewHistory(file);
      break;
    case "rename":
      void handleRenameEntry(file);
      break;
    case "move":
      handleMoveEntry(file);
      break;
    case "transfer":
      openTransferDialog([file]);
      break;
    case "download":
      handleDownload(file);
      break;
    case "share":
      handleShare(file);
      break;
    case "delete":
      void handleDelete(file);
      break;
  }
}

/** 点击表头：切换字段时改字段，重复点击同一字段时切换升降序。 */
function handleSortChange(field: SortField) {
  if (fileView.sortField === field) {
    fileView.toggleSortDirection();
    return;
  }
  fileView.setSortField(field);
}
</script>

<style scoped>
.file-browser {
  /* 指向全局设计令牌，随明暗主题切换 */
  /* 唯一仍被引用的 explorer 令牌（搜索面板 ::after 分隔线）；
     其余 accent/panel-bg/shell-bg/list-bg 消费者已在历轮重构中移除，令牌随之清理。 */
  --explorer-panel-border: var(--vf-border);
  margin: 0 auto;
  padding: 0;
}

/* 单一内容面板：内部区块之间只用细分隔线，不再层层嵌套卡片 */
.file-browser-box {
  display: flex;
  flex-direction: column;
  border-radius: var(--vf-radius-lg);
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

  .desktop-list-shell {
    flex: 1 1 auto;
    min-height: 0;
    /* 列表区域是唯一滚动容器：行、加载更多提示一起滚动，
       列头用 sticky 常驻，与主流网盘一致 */
    overflow: auto;
    scrollbar-gutter: stable;
    /* sticky 表头的滚动缓冲（38px 表头高）：scrollIntoView/键盘导航停在表头下方，
       否则向上导航时活动行会被吸进表头（实测藏于表头下 38px）。 */
    scroll-padding-top: 2.375rem;
  }

  /* thead 由子组件 FileList.vue 渲染，需 :deep() 才能匹配到 th（此前规则从未生效） */
  .desktop-list-shell :deep(thead th) {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--vf-surface);
  }
}

.breadcrumb-bar {
  padding: 0.85rem 1.1rem;
  border-bottom: 1px solid var(--vf-border-weak);
}

.file-browser-toolbar {
  position: relative;
  z-index: 2;
  padding: 0.6rem 1.1rem;
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
  font-size: 0.78rem;
}

/* 状态栏退化为一行低调的说明文字 */
.desktop-status-bar {
  display: flex;
  align-items: center;
  gap: 1rem;
  flex-wrap: wrap;
  padding: 0.55rem 1.1rem;
  border-top: 1px solid var(--vf-border-weak);
  color: var(--vf-text-muted);
  font-size: 0.78rem;
}

.desktop-status-shortcuts {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.1rem 0.35rem;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: inherit;
  font-size: inherit;
  cursor: pointer;
}

.desktop-status-shortcuts:hover,
.desktop-status-shortcuts:focus-visible {
  background: var(--vf-surface-hover);
  color: var(--vf-text);
}

.desktop-status-typeahead {
  color: var(--vf-accent);
}

.desktop-status-typeahead strong {
  font-weight: 600;
}

.desktop-status-shortcuts-more {
  color: var(--vf-accent);
}

/* 提示靠右显示，保持状态栏左侧信息优先 */
.desktop-status-shortcuts {
  margin-left: auto;
  color: var(--vf-text-subtle);
  white-space: nowrap;
}

/* 桌面内容区：列表 + 右侧详细信息面板 */
.desktop-content-layout {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  min-height: 0;
}

.desktop-content-layout.has-tree {
  grid-template-columns: 232px minmax(0, 1fr);
}

/* 上传进度胶囊：与「上传」按钮并排，显示队列进度 */
.upload-indicator {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  font-size: 0.78rem;
  color: var(--vf-text-muted);
}

.upload-indicator-dot {
  width: 0.45rem;
  height: 0.45rem;
  border-radius: 50%;
  background: var(--vf-accent);
  animation: upload-indicator-pulse 1.2s var(--vf-motion-standard) infinite;
}

@keyframes upload-indicator-pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.35;
  }
}

@media (prefers-reduced-motion: reduce) {
  .upload-indicator-dot {
    animation: none;
  }
}

/* 侧栏 = 目录树（可滚动）+ 概览（固定高度） */
.browser-sidebar {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  /* 防御：任何意外内容都在侧栏内裁剪，不得盖到状态栏 */
  overflow: hidden;
}

.desktop-content-layout.has-tree.has-details {
  grid-template-columns: 232px minmax(0, 1fr) 280px;
}

.desktop-content-layout.has-details {
  grid-template-columns: minmax(0, 1fr) 280px;
}

@media screen and (max-width: 1023px) {
  .file-browser {
    padding: 0;
  }

  .file-browser-box {
    padding: 0.5rem;
    border-radius: 0;
    box-shadow: none;
    border-left: none;
    border-right: none;
  }

  .file-browser-toolbar {
    padding: 0.55rem 0.9rem;
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
    margin-bottom: 0.5rem;
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
  padding: 0.55rem 1.1rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius);
  background: var(--vf-surface-raised);
  box-shadow: var(--vf-shadow-menu);
  color: var(--vf-text-strong);
  font-size: 1.5rem;
  font-weight: 600;
  letter-spacing: 0.04em;
  pointer-events: none;
  transition: opacity 0.12s var(--vf-motion-standard);
}

.desktop-drag-chip.is-invalid {
  background: var(--vf-danger-soft-strong);
  border-color: var(--vf-danger-line);
}

.desktop-drag-chip.is-over-target {
  background: var(--vf-accent-soft-strong);
  border-color: var(--vf-accent);
}

.desktop-drag-chip {
  position: fixed;
  z-index: 60;
  padding: 0.35rem 0.75rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface-raised);
  box-shadow: var(--vf-shadow-menu);
  color: var(--vf-text-strong);
  font-size: 0.78rem;
  font-weight: 600;
  pointer-events: none;
  white-space: nowrap;
}
</style>
