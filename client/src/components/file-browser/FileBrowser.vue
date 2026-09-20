<template>
  <div
    class="file-browser"
    @touchstart="onTouchStart"
    @touchmove="onTouchMove"
    @touchend="onTouchEnd"
  >
    <div class="box file-browser-box">
      <div class="breadcrumb-bar">
        <div class="breadcrumb-current-path" :title="currentPathLabel">
          {{ currentPathLabel }}
        </div>
      </div>

      <div class="file-browser-toolbar">
        <template v-if="isMobile">
          <div
            v-if="pullIndicatorVisible"
            class="has-text-centered is-size-7 has-text-grey mb-2"
          >
            <span v-if="pullRefreshing">刷新中...</span>
            <span v-else-if="pullReady">释放刷新</span>
            <span v-else>下拉刷新</span>
          </div>

          <div class="field has-addons mt-3">
            <div class="control is-expanded">
              <input
                v-model="searchQuery"
                class="input"
                type="text"
                :placeholder="
                  searchMode === 'content' ? '搜索文件内容...' : '搜索文件名...'
                "
                list="vfiles-search-history"
                @keyup.enter="runSearch"
              />
              <datalist id="vfiles-search-history">
                <option
                  v-for="item in searchHistory"
                  :key="item"
                  :value="item"
                />
              </datalist>
            </div>
            <div class="control">
              <button
                class="button is-link"
                :class="{ 'is-loading': searchLoading }"
                :disabled="searchLoading"
                @click="runSearch"
              >
                <IconSearch :size="20" />
              </button>
            </div>
            <div class="control">
              <button
                class="button"
                :disabled="searchLoading"
                @click="clearSearch"
              >
                清空
              </button>
            </div>
          </div>

          <div class="field mt-2">
            <label class="checkbox">
              <input
                type="checkbox"
                v-model="searchContent"
                :disabled="searchLoading || !searchContentEnabled"
              />
              全文
            </label>
            <p v-if="!searchContentEnabled" class="help is-warning">
              内容搜索功能未启用
            </p>
          </div>

          <div class="field is-grouped is-grouped-multiline mt-2">
            <div class="control">
              <div class="select is-small">
                <select v-model="searchType" :disabled="searchLoading">
                  <option value="all">全部</option>
                  <option value="file">仅文件</option>
                  <option value="directory">仅文件夹</option>
                </select>
              </div>
            </div>

            <div class="control">
              <label class="checkbox">
                <input
                  type="checkbox"
                  v-model="searchScopeCurrent"
                  :disabled="searchLoading"
                />
                仅当前目录
              </label>
            </div>
          </div>

          <div v-if="searchError" class="notification is-danger is-light">
            <IconAlertCircle :size="20" class="mr-2" />
            {{ searchError }}
          </div>
        </template>

        <template v-else>
          <div class="desktop-command-bar">
            <div class="desktop-command-group">
              <button
                class="button is-small is-light desktop-command-button"
                :disabled="!currentPath"
                @click="goBack"
              >
                <IconArrowLeft :size="16" />
                <span>上一级</span>
              </button>
              <button
                class="button is-small is-light desktop-command-button"
                @click="refresh"
              >
                <IconRefresh :size="16" />
                <span>刷新</span>
              </button>
              <button
                class="button is-small is-primary desktop-command-button"
                @click="showUploader = true"
              >
                <IconUpload :size="16" />
                <span>上传文件</span>
              </button>
              <button
                class="button is-small desktop-command-button"
                :class="batchMode ? 'is-link is-light' : 'is-light'"
                @click="toggleBatchMode"
              >
                <IconChecklist :size="16" />
                <span>{{ batchMode ? "退出批量" : "批量选择" }}</span>
              </button>
              <ViewOptions />
              <div ref="desktopSearchBoxRef" class="desktop-search-box">
                <div class="desktop-search-inline">
                  <div class="control desktop-search-field">
                    <input
                      ref="desktopSearchInputRef"
                      v-model="searchQuery"
                      class="input is-small desktop-search-control"
                      type="text"
                      :placeholder="
                        searchMode === 'content'
                          ? '搜索当前工作区中的文本内容'
                          : '搜索名称、扩展名或路径'
                      "
                      list="vfiles-search-history-desktop"
                      @keyup.enter="runDesktopSearch"
                    />
                    <datalist id="vfiles-search-history-desktop">
                      <option
                        v-for="item in searchHistory"
                        :key="item"
                        :value="item"
                      />
                    </datalist>
                  </div>

                  <div class="desktop-search-action-group">
                    <button
                      class="button is-small desktop-command-button desktop-search-button"
                      :class="[
                        searchActive ? 'is-link is-light' : 'is-light',
                        { 'is-loading': searchLoading },
                      ]"
                      :disabled="searchLoading"
                      @click="runDesktopSearch"
                    >
                      <IconSearch :size="16" />
                      <span>搜索</span>
                    </button>
                    <button
                      class="button is-small desktop-command-button desktop-search-toggle"
                      :class="[
                        desktopSearchOpen || desktopSearchFiltersActive
                          ? 'is-link is-light'
                          : 'is-light',
                        { 'is-open': desktopSearchOpen },
                      ]"
                      title="高级搜索"
                      aria-label="高级搜索"
                      :aria-expanded="desktopSearchOpen"
                      @click="toggleDesktopSearch"
                    >
                      <IconChevronDown :size="16" />
                    </button>
                  </div>
                </div>

                <div
                  v-if="desktopSearchOpen"
                  class="desktop-search-panel desktop-search-panel--dropdown"
                >
                  <div class="desktop-search-panel-heading">高级搜索</div>

                  <div class="desktop-search-filters">
                    <label class="checkbox desktop-filter-pill">
                      <input
                        type="checkbox"
                        v-model="searchContent"
                        :disabled="searchLoading || !searchContentEnabled"
                      />
                      全文搜索
                    </label>
                    <p
                      v-if="!searchContentEnabled"
                      class="is-size-7 has-text-warning ml-2"
                    >
                      (未启用)
                    </p>

                    <div class="select is-small desktop-filter-select">
                      <select v-model="searchType" :disabled="searchLoading">
                        <option value="all">全部</option>
                        <option value="file">仅文件</option>
                        <option value="directory">仅文件夹</option>
                      </select>
                    </div>

                    <label class="checkbox desktop-filter-pill">
                      <input
                        type="checkbox"
                        v-model="searchScopeCurrent"
                        :disabled="searchLoading"
                      />
                      仅当前目录
                    </label>
                  </div>

                  <div class="desktop-search-dropdown-actions">
                    <button
                      class="button is-small is-light desktop-command-button desktop-search-clear"
                      :disabled="searchLoading"
                      @click="clearDesktopSearch"
                    >
                      清空搜索
                    </button>
                  </div>
                </div>
              </div>
            </div>

            <div v-if="searchError" class="notification is-danger is-light">
              <IconAlertCircle :size="18" class="mr-2" />
              {{ searchError }}
            </div>
          </div>

          <div v-if="batchMode" class="desktop-batch-strip">
            <div class="desktop-batch-meta">已选 {{ selectedCount }} 项</div>
            <div class="desktop-batch-actions">
              <button
                class="button is-small is-light"
                @click="selectAllVisible"
              >
                全选当前视图
              </button>
              <button class="button is-small is-light" @click="clearSelection">
                清空选择
              </button>
              <button
                class="button is-small is-info"
                :disabled="selectedCount === 0"
                @click="batchDownload"
              >
                批量下载
              </button>
              <button
                class="button is-small is-danger is-light"
                :disabled="selectedCount === 0"
                @click="batchDelete"
              >
                删除
              </button>
              <button
                class="button is-small is-light"
                :disabled="selectedCount === 0"
                @click="batchMove"
              >
                移动
              </button>
              <button
                class="button is-small is-light"
                :disabled="selectedCount !== 1"
                @click="renameSelected"
              >
                重命名
              </button>
            </div>
          </div>

          <div v-if="searchError" class="notification is-danger is-light mb-3">
            <IconAlertCircle :size="20" class="mr-2" />
            {{ searchError }}
          </div>
        </template>
      </div>

      <div v-if="downloadQueue.length" class="box mb-4">
        <div class="level is-mobile">
          <div class="level-left">
            <div class="level-item">
              <div>
                <p class="heading">下载队列</p>
                <p class="title is-6">
                  {{ downloadQueue.length }} 项
                  <span v-if="downloading" class="tag is-info is-light ml-2"
                    >下载中</span
                  >
                  <span
                    v-if="queueCollapsed && activeDownload"
                    class="tag is-light ml-2 is-size-7"
                  >
                    {{ activeDownload.filename }}
                    <span v-if="activeDownloadPercent != null">
                      · {{ activeDownloadPercent }}%</span
                    >
                  </span>
                </p>
              </div>
            </div>
          </div>
          <div class="level-right">
            <div class="level-item">
              <div class="buttons">
                <button
                  class="button is-small is-light"
                  @click="toggleQueuePanel"
                  :disabled="!downloadQueue.length"
                >
                  {{ queueCollapsed ? "展开" : "最小化" }}
                </button>
                <button
                  class="button is-small is-light"
                  @click="clearFinished"
                  :disabled="downloading && downloadQueue.length === 1"
                >
                  清空已完成
                </button>
                <button
                  class="button is-small is-danger is-light"
                  @click="cancelAll"
                  :disabled="!downloadQueue.length"
                >
                  全部取消
                </button>
              </div>
            </div>
          </div>
        </div>

        <div v-if="!queueCollapsed" class="content">
          <div
            v-for="item in downloadQueue"
            :key="item.id"
            class="download-item"
          >
            <div
              class="is-flex is-justify-content-space-between is-align-items-center"
            >
              <div class="mr-2" style="min-width: 0">
                <strong class="is-size-7">{{ item.filename }}</strong>
                <span class="tag is-light ml-2 is-size-7">{{
                  item.kind === "folder" ? "ZIP" : "文件"
                }}</span>
                <span
                  v-if="item.status === 'queued'"
                  class="tag is-light ml-2 is-size-7"
                  >排队中</span
                >
                <span
                  v-else-if="item.status === 'downloading'"
                  class="tag is-info is-light ml-2 is-size-7"
                  >下载中
                  <template v-if="item.progress?.total">
                    {{
                      formatProgress(item.progress.loaded, item.progress.total)
                    }}
                  </template>
                </span>
                <span
                  v-else-if="item.status === 'done'"
                  class="tag is-success is-light ml-2 is-size-7"
                  >完成</span
                >
                <span
                  v-else-if="item.status === 'canceled'"
                  class="tag is-warning is-light ml-2 is-size-7"
                  >已取消</span
                >
                <span
                  v-else-if="item.status === 'error'"
                  class="tag is-danger is-light ml-2 is-size-7"
                  >失败</span
                >
              </div>

              <div class="buttons is-right">
                <button
                  v-if="
                    item.status === 'queued' || item.status === 'downloading'
                  "
                  class="button is-small is-light"
                  @click="cancelItem(item.id)"
                >
                  取消
                </button>
                <button
                  v-else
                  class="button is-small is-light"
                  @click="removeItem(item.id)"
                >
                  移除
                </button>
              </div>
            </div>

            <progress
              v-if="
                item.status === 'downloading' &&
                item.progress &&
                item.progress.total
              "
              class="progress is-small is-info mt-2"
              :value="item.progress.loaded"
              :max="item.progress.total"
            ></progress>
            <progress
              v-else-if="item.status === 'downloading'"
              class="progress is-small is-info mt-2"
              max="100"
            ></progress>

            <p v-if="item.error" class="has-text-danger is-size-7 mt-1">
              {{ item.error }}
            </p>
          </div>
        </div>
      </div>

      <template v-if="!isMobile">
        <div class="desktop-list-primary-shell">
          <div class="desktop-list-shell">
            <div v-if="loading" class="has-text-centered py-6">
              <div class="spinner mb-3"></div>
              <p class="has-text-grey">加载中...</p>
            </div>

            <div v-else-if="error" class="notification is-danger is-light">
              <IconAlertCircle :size="20" class="mr-2" />
              {{ error }}
            </div>

            <div
              v-else-if="!searchActive && files.length === 0"
              class="has-text-centered py-6"
            >
              <IconFolderOpen :size="64" class="has-text-grey-light mb-3" />
              <p class="has-text-grey">此文件夹为空</p>
            </div>

            <div
              v-else-if="searchActive && searchResults.length === 0"
              class="has-text-centered py-6"
            >
              <p class="has-text-grey">没有找到匹配的文件</p>
            </div>

            <template v-else>
              <div v-if="searchActive" class="desktop-list-meta">
                搜索结果：{{ searchResults.length }} 项（{{
                  searchMode === "content" ? "内容" : "文件名"
                }}）
              </div>

              <FileGrid
                v-if="viewMode === 'grid'"
                :files="desktopItems"
                :highlight="searchActive ? searchQuery : ''"
                :commit="browseCommit"
                :select-mode="batchMode"
                :selected-paths="selectedPaths"
                :active-path="desktopActivePath"
                :thumbnail-size="fileView.thumbnailSize"
                @click="handleItemClick"
                @download="handleDownload"
                @rename="handleRenameEntry"
                @move="handleMoveEntry"
                @delete="handleDelete"
                @view-history="handleViewHistory"
                @toggle-select="toggleSelect"
                @modifier-select="handleModifierSelect"
                @context-menu="handleContextMenu"
                @share="handleShare"
                @preview="handlePreview"
                @open-folder="handleOpenFolder"
                @create-directory="handleCreateDirectory"
              />
              <FileList
                v-else
                :files="desktopItems"
                :highlight="searchActive ? searchQuery : ''"
                :select-mode="batchMode"
                :selected-paths="selectedPaths"
                :expanded-path="expandedFilePath"
                :active-path="desktopActivePath"
                :desktop="true"
                :sort-field="fileView.sortField"
                :sort-direction="fileView.sortDirection"
                @click="handleItemClick"
                @download="handleDownload"
                @rename="handleRenameEntry"
                @move="handleMoveEntry"
                @delete="handleDelete"
                @view-history="handleViewHistory"
                @toggle-select="toggleSelect"
                @modifier-select="handleModifierSelect"
                @context-menu="handleContextMenu"
                @toggle-select-all="toggleSelectAll"
                @sort-change="handleSortChange"
                @share="handleShare"
                @preview="handlePreview"
                @open-folder="handleOpenFolder"
                @create-directory="handleCreateDirectory"
              />
            </template>
          </div>

          <div class="desktop-status-bar">
            <span>{{
              searchActive
                ? `搜索结果 ${searchResults.length} 项`
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
            <span class="desktop-status-shortcuts is-hidden-touch">
              Ctrl/⌘+A 全选 · Delete 删除 · F2 重命名 · Enter 打开 · Esc 退出
            </span>
          </div>
        </div>
      </template>

      <template v-else>
        <div v-if="loading" class="has-text-centered py-6">
          <div class="spinner mb-3"></div>
          <p class="has-text-grey">加载中...</p>
        </div>

        <div v-else-if="error" class="notification is-danger is-light">
          <IconAlertCircle :size="20" class="mr-2" />
          {{ error }}
        </div>

        <div
          v-else-if="!searchActive && files.length === 0"
          class="has-text-centered py-6"
        >
          <IconFolderOpen :size="64" class="has-text-grey-light mb-3" />
          <p class="has-text-grey">此文件夹为空</p>
        </div>

        <div v-else-if="searchActive" class="file-list">
          <p class="has-text-grey is-size-7 mb-2">
            搜索结果：{{ searchResults.length }} 项（{{
              searchMode === "content" ? "内容" : "文件名"
            }}）
          </p>
          <div v-if="searchResults.length === 0" class="has-text-centered py-6">
            <p class="has-text-grey">没有找到匹配的文件</p>
          </div>
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
            @move="handleMoveEntry"
            @delete="handleDelete"
            @view-history="handleViewHistory"
            @toggle-select="toggleSelect"
            @modifier-select="handleModifierSelect"
            @context-menu="handleContextMenu"
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
            :expanded-path="expandedFilePath"
            @click="handleItemClick"
            @download="handleDownload"
            @rename="handleRenameEntry"
            @move="handleMoveEntry"
            @delete="handleDelete"
            @view-history="handleViewHistory"
            @toggle-select="toggleSelect"
            @modifier-select="handleModifierSelect"
            @context-menu="handleContextMenu"
            @share="handleShare"
            @preview="handlePreview"
            @open-folder="handleOpenFolder"
            @create-directory="handleCreateDirectory"
          />

          <div
            v-if="isMobile && hasMore"
            ref="loadMoreSentinel"
            class="has-text-centered has-text-grey is-size-7 py-2"
          >
            继续下滑加载更多...
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
            @move="handleMoveEntry"
            @delete="handleDelete"
            @view-history="handleViewHistory"
            @toggle-select="toggleSelect"
            @modifier-select="handleModifierSelect"
            @context-menu="handleContextMenu"
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
            :expanded-path="expandedFilePath"
            @click="handleItemClick"
            @download="handleDownload"
            @rename="handleRenameEntry"
            @move="handleMoveEntry"
            @delete="handleDelete"
            @view-history="handleViewHistory"
            @toggle-select="toggleSelect"
            @modifier-select="handleModifierSelect"
            @context-menu="handleContextMenu"
            @share="handleShare"
            @preview="handlePreview"
            @open-folder="handleOpenFolder"
            @create-directory="handleCreateDirectory"
          />

          <div
            v-if="isMobile && hasMore"
            ref="loadMoreSentinel"
            class="has-text-centered has-text-grey is-size-7 py-2"
          >
            继续下滑加载更多...
          </div>
        </div>
      </template>
    </div>

    <!-- 上传对话框 -->
    <Modal
      :show="showUploader"
      title="上传文件"
      :mobile-compact="true"
      @close="showUploader = false"
    >
      <FileUploader
        ref="fileUploaderRef"
        :target-path="filesStore.currentPath"
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
        <p v-if="!currentPath" class="has-text-grey is-size-7">
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
        <p v-if="!currentPath" class="has-text-grey is-size-7">
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
      @close="showHistory = false"
    >
      <VersionHistory v-if="selectedFile" :file-path="selectedFile.path" />
    </Modal>

    <!-- 分享对话框 -->
    <ShareDialog
      :is-active="showShareDialog"
      :file-path="selectedFile?.path || ''"
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
      :show="preview.open"
      :title="`预览: ${previewFilename}`"
      :mobile-compact="true"
      @close="closePreview"
    >
      <div v-if="preview.loading" class="has-text-centered py-6">
        <div class="spinner mb-3"></div>
        <p class="has-text-grey">加载预览中...</p>
      </div>

      <div v-else-if="preview.error" class="notification is-danger is-light">
        {{ preview.error }}
      </div>

      <div v-else>
        <figure v-if="preview.kind === 'image'" class="image">
          <img
            :src="preview.objectUrl"
            :alt="previewFilename"
            loading="lazy"
            decoding="async"
          />
        </figure>

        <div v-else-if="preview.kind === 'pdf'" class="preview-frame">
          <iframe
            :src="preview.objectUrl"
            title="PDF 预览"
            class="preview-iframe"
          />
        </div>

        <div v-else-if="preview.kind === 'video'" class="preview-media">
          <video :src="preview.objectUrl" controls class="preview-video" />
        </div>

        <div v-else-if="preview.kind === 'audio'" class="preview-media">
          <audio :src="preview.objectUrl" controls class="preview-audio" />
        </div>

        <div
          v-else-if="preview.kind === 'markdown'"
          class="content markdown-body"
          v-html="preview.html"
        ></div>

        <div v-else-if="preview.kind === 'code'" class="content">
          <pre
            class="preview-code hljs"
          ><code v-html="preview.html"></code></pre>
        </div>

        <div v-else-if="preview.kind === 'text'" class="content">
          <pre class="preview-text">{{ preview.text }}</pre>
        </div>

        <div v-else class="notification is-warning is-light">
          暂不支持该文件类型的在线预览，请使用下载。
        </div>
      </div>
    </Modal>
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
  IconChevronDown,
  IconArrowLeft,
  IconChecklist,
  IconRefresh,
  IconUpload,
  IconEye,
  IconHistory,
  IconPencil,
  IconArrowsDiff,
  IconDownload,
  IconShare,
  IconTrash,
} from "@tabler/icons-vue";
import { useFilesStore } from "../../stores/files.store";
import { useAppStore } from "../../stores/app.store";
import { useAuthStore } from "../../stores/auth.store";
import { useFileViewStore } from "../../stores/fileView.store";
import { filesService } from "../../services/files.service";
import FileList from "./FileList.vue";
import FileGrid from "./FileGrid.vue";
import ViewOptions from "./ViewOptions.vue";
import ContextMenu, { type ContextMenuItem } from "./ContextMenu.vue";
import MoveDialog from "./MoveDialog.vue";
import FileUploader from "../file-uploader/FileUploader.vue";
import VersionHistory from "../version-history/VersionHistory.vue";
import Modal from "../common/Modal.vue";
import ShareDialog from "../common/ShareDialog.vue";
import { confirmDialog, promptDialog } from "../../composables/dialog";
import type { FileInfo } from "../../types";
import { loadHighlight } from "../../utils/highlight";
import {
  sortBrowserItems,
  sortFiles,
  type SortField,
  type SortState,
} from "../../utils/fileSort";

let cachedMarked: any | null = null;
let cachedHljs: any | null = null;

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

const currentPathLabel = computed(() => {
  return currentPath.value ? `/${currentPath.value}` : "根目录";
});

const isMobile = ref(false);

const MOBILE_LAYOUT_MEDIA_QUERY = "(max-width: 1023px)";

function updateIsMobile() {
  isMobile.value = window.matchMedia(MOBILE_LAYOUT_MEDIA_QUERY).matches;
}

const showUploader = ref(false);
const showHistory = ref(false);
const showShareDialog = ref(false);
const showMoveDialog = ref(false);
const selectedFile = ref<FileInfo | null>(null);
const moveDialogItems = ref<FileInfo[]>([]);
const moveDialogInitialPath = ref("");
const moveDialogSubmitting = ref(false);
const fileUploaderRef = ref<InstanceType<typeof FileUploader> | null>(null);
const expandedFilePath = ref<string>("");

type PreviewKind =
  | "text"
  | "image"
  | "markdown"
  | "code"
  | "pdf"
  | "video"
  | "audio"
  | "unsupported";
const preview = ref({
  open: false,
  loading: false,
  error: null as string | null,
  path: "",
  kind: "text" as PreviewKind,
  text: "",
  html: "",
  objectUrl: "",
});

const previewFilename = computed(
  () => preview.value.path.split("/").pop() || "file",
);

function getExtension(p: string): string {
  const name = p.split("/").pop() || "";
  const idx = name.lastIndexOf(".");
  if (idx <= 0 || idx === name.length - 1) return "";
  return name.slice(idx + 1).toLowerCase();
}

function detectPreviewKind(filePath: string): PreviewKind {
  const ext = getExtension(filePath);
  const imageExts = new Set([
    "png",
    "jpg",
    "jpeg",
    "gif",
    "webp",
    "bmp",
    "svg",
  ]);
  if (imageExts.has(ext)) return "image";

  if (ext === "pdf") return "pdf";

  const videoExts = new Set(["mp4", "webm", "ogg", "mov", "m4v"]);
  if (videoExts.has(ext)) return "video";

  const audioExts = new Set(["mp3", "wav", "ogg", "m4a", "aac", "flac"]);
  if (audioExts.has(ext)) return "audio";

  const mdExts = new Set(["md", "markdown"]);
  if (mdExts.has(ext)) return "markdown";

  const codeExts = new Set([
    "js",
    "ts",
    "jsx",
    "tsx",
    "vue",
    "json",
    "css",
    "scss",
    "html",
    "xml",
    "yml",
    "yaml",
    "csv",
    "log",
    "sh",
    "py",
    "java",
    "c",
    "cpp",
    "go",
    "rs",
  ]);
  if (codeExts.has(ext)) return "code";

  const textExts = new Set(["txt", "log"]);
  if (textExts.has(ext) || ext === "") return "text";

  return "unsupported";
}

function guessMimeByExt(filePath: string): string {
  const ext = getExtension(filePath);
  if (ext === "pdf") return "application/pdf";
  if (ext === "svg") return "image/svg+xml";
  if (ext === "png") return "image/png";
  if (ext === "jpg" || ext === "jpeg") return "image/jpeg";
  if (ext === "gif") return "image/gif";
  if (ext === "webp") return "image/webp";
  if (ext === "bmp") return "image/bmp";

  if (ext === "mp4" || ext === "m4v") return "video/mp4";
  if (ext === "webm") return "video/webm";
  if (ext === "mov") return "video/quicktime";
  if (ext === "ogg") return "application/ogg";

  if (ext === "mp3") return "audio/mpeg";
  if (ext === "wav") return "audio/wav";
  if (ext === "m4a") return "audio/mp4";
  if (ext === "aac") return "audio/aac";
  if (ext === "flac") return "audio/flac";

  return "application/octet-stream";
}

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function safeLinkHref(href: string | null | undefined): string {
  const raw = (href || "").trim();
  if (!raw) return "#";
  if (raw.startsWith("#")) return raw;
  if (raw.startsWith("/")) return raw;
  if (/^https?:\/\//i.test(raw)) return raw;
  if (/^mailto:/i.test(raw)) return raw;
  return "#";
}

function safeImageSrc(src: string | null | undefined): string {
  const raw = (src || "").trim();
  if (!raw) return "";
  if (/^https?:\/\//i.test(raw)) return raw;
  if (/^data:image\//i.test(raw)) return raw;
  if (raw.startsWith("/")) return raw;
  return "";
}

async function getMarked() {
  if (cachedMarked) return cachedMarked;
  const mod: any = await import("marked");
  const markedApi = mod?.marked ?? mod;

  const mdRenderer: any = {
    html(token: any) {
      const html =
        typeof token === "string" ? token : (token?.text ?? token?.raw ?? "");
      return escapeHtml(String(html));
    },
    link(tokenOrHref: any, title?: any, text?: any) {
      const href =
        tokenOrHref && typeof tokenOrHref === "object"
          ? tokenOrHref.href
          : tokenOrHref;
      const linkTitle =
        tokenOrHref && typeof tokenOrHref === "object"
          ? tokenOrHref.title
          : title;
      const linkText =
        tokenOrHref && typeof tokenOrHref === "object"
          ? tokenOrHref.text
          : text;

      const safeHref = safeLinkHref(href);
      const t = linkTitle ? ` title="${escapeHtml(String(linkTitle))}"` : "";
      const inner =
        typeof linkText === "string"
          ? (markedApi.parseInline(linkText) as string)
          : "";
      return `<a href="${escapeHtml(safeHref)}"${t} target="_blank" rel="noopener noreferrer">${inner}</a>`;
    },
    image(tokenOrHref: any, title?: any, text?: any) {
      const href =
        tokenOrHref && typeof tokenOrHref === "object"
          ? tokenOrHref.href
          : tokenOrHref;
      const imgTitle =
        tokenOrHref && typeof tokenOrHref === "object"
          ? tokenOrHref.title
          : title;
      const altText =
        tokenOrHref && typeof tokenOrHref === "object"
          ? tokenOrHref.text
          : text;

      const safeSrc = safeImageSrc(href);
      if (!safeSrc) return "";

      const t = imgTitle ? ` title="${escapeHtml(String(imgTitle))}"` : "";
      const alt = altText ? escapeHtml(String(altText)) : "";
      return `<img src="${escapeHtml(safeSrc)}" alt="${alt}" loading="lazy" decoding="async"${t} />`;
    },
  };

  markedApi.use({
    renderer: mdRenderer,
    gfm: true,
    breaks: true,
  });

  cachedMarked = markedApi;
  return markedApi;
}

async function getHljs() {
  if (cachedHljs) return cachedHljs;
  cachedHljs = await loadHighlight();
  return cachedHljs;
}

function closePreview() {
  if (preview.value.objectUrl) URL.revokeObjectURL(preview.value.objectUrl);
  preview.value = {
    open: false,
    loading: false,
    error: null,
    path: "",
    kind: "text",
    text: "",
    html: "",
    objectUrl: "",
  };
}

async function openPreview(filePath: string) {
  closePreview();
  preview.value.open = true;
  preview.value.loading = true;
  preview.value.path = filePath;
  preview.value.kind = detectPreviewKind(filePath);

  try {
    if (preview.value.kind === "unsupported") {
      preview.value.loading = false;
      return;
    }

    const blob = await filesService.getFileContent(
      filePath,
      browseCommit.value,
    );

    if (
      preview.value.kind === "image" ||
      preview.value.kind === "pdf" ||
      preview.value.kind === "video" ||
      preview.value.kind === "audio"
    ) {
      const typed = new Blob([await blob.arrayBuffer()], {
        type: guessMimeByExt(filePath),
      });
      preview.value.objectUrl = URL.createObjectURL(typed);
    } else {
      const text = await blob.text();
      if (preview.value.kind === "markdown") {
        const markedApi = await getMarked();
        preview.value.html = markedApi.parse(text) as string;
      } else if (preview.value.kind === "code") {
        const hljsApi = await getHljs();
        const highlighted = hljsApi.highlightAuto(text);
        preview.value.html = highlighted.value;
      } else {
        preview.value.text = text;
      }
    }
  } catch (err) {
    preview.value.error = err instanceof Error ? err.message : "预览失败";
  } finally {
    preview.value.loading = false;
  }
}

const searchQuery = ref("");
const searchResults = ref<FileInfo[]>([]);
const searchLoading = ref(false);
const searchError = ref<string | null>(null);
const searchActive = ref(false);
const searchContent = ref(false);
const desktopSearchOpen = ref(false);
const desktopSearchBoxRef = ref<HTMLElement | null>(null);
const desktopSearchInputRef = ref<HTMLInputElement | null>(null);

const queueCollapsed = ref(false);
const activeDownload = computed(() =>
  downloadQueue.value.find((x) => x.status === "downloading"),
);
const activeDownloadPercent = computed(() => {
  const a = activeDownload.value;
  if (!a?.progress?.total) return null;
  if (a.progress.total <= 0) return null;
  return Math.min(
    100,
    Math.floor((a.progress.loaded / a.progress.total) * 100),
  );
});

type DownloadQueueStatus =
  | "queued"
  | "downloading"
  | "done"
  | "error"
  | "canceled";
type DownloadQueueKind = "file" | "folder";
type DownloadQueueItem = {
  id: number;
  kind: DownloadQueueKind;
  path: string;
  filename: string;
  status: DownloadQueueStatus;
  progress?: { loaded: number; total?: number };
  error?: string;
  abort?: AbortController;
};

const downloadQueue = ref<DownloadQueueItem[]>([]);
let nextDownloadId = 1;
const downloading = computed(() =>
  downloadQueue.value.some((x) => x.status === "downloading"),
);

const batchMode = ref(false);
const selectedPaths = ref<Set<string>>(new Set());
/** 最近一次点击的条目路径，用于 Shift 范围选择。 */
const lastSelectedPath = ref<string>("");

const selectedCount = computed(() => selectedPaths.value.size);

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
  items.push({ key: "rename", label: "重命名", icon: IconPencil });
  items.push({ key: "move", label: "移动", icon: IconArrowsDiff });
  items.push({ key: "download", label: "下载", icon: IconDownload });
  items.push({ key: "share", label: "分享", icon: IconShare });
  items.push({ key: "delete", label: "删除", icon: IconTrash, danger: true });
  return items;
});

const searchType = ref<"all" | "file" | "directory">("all");
const searchScopeCurrent = ref(false);

const searchMode = computed(() => (searchContent.value ? "content" : "name"));
const desktopSearchFiltersActive = computed(
  () =>
    searchContent.value ||
    searchType.value !== "all" ||
    searchScopeCurrent.value,
);

const SEARCH_HISTORY_KEY = "vfiles.searchHistory";
const searchHistory = ref<string[]>([]);

function loadSearchHistory() {
  try {
    const raw = localStorage.getItem(SEARCH_HISTORY_KEY);
    if (!raw) return;
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      searchHistory.value = parsed
        .filter((x) => typeof x === "string")
        .slice(0, 10);
    }
  } catch {
    // ignore
  }
}

function saveSearchHistory(next: string[]) {
  searchHistory.value = next;
  try {
    localStorage.setItem(SEARCH_HISTORY_KEY, JSON.stringify(next));
  } catch {
    // ignore
  }
}

function closeDesktopSearch() {
  desktopSearchOpen.value = false;
}

function toggleDesktopSearch() {
  desktopSearchOpen.value = !desktopSearchOpen.value;
}

async function runDesktopSearch() {
  const q = searchQuery.value.trim();
  if (!q) {
    clearSearch();
    closeDesktopSearch();
    void nextTick().then(() => desktopSearchInputRef.value?.focus());
    return;
  }

  await runSearch();
  if (!searchError.value) closeDesktopSearch();
}

function clearDesktopSearch() {
  clearSearch();
  closeDesktopSearch();
  void nextTick().then(() => desktopSearchInputRef.value?.focus());
}

function pushSearchHistory(term: string) {
  const value = term.trim();
  if (!value) return;
  const normalized = value;

  const withoutDup = searchHistory.value.filter(
    (x) => x.toLowerCase() !== normalized.toLowerCase(),
  );
  saveSearchHistory([normalized, ...withoutDup].slice(0, 10));
}

onMounted(() => {
  filesStore.loadFiles();
  loadSearchHistory();

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

    // Escape 逐层退出：高级搜索 → 预览 → 批量模式 → 选择
    if (e.key === "Escape") {
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
  try {
    const mql = window.matchMedia(MOBILE_LAYOUT_MEDIA_QUERY);
    const handler = () => updateIsMobile();
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
    // ignore
  }
});

onBeforeUnmount(() => {
  closePreview();
});

function navigateTo(path: string) {
  expandedFilePath.value = "";
  filesStore.navigateTo(path);
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

const dirManagerOpen = ref(false);
const dirOpLoading = ref<null | "create" | "rename" | "delete">(null);
const dirOpBusy = computed(() => dirOpLoading.value !== null);
const newDirName = ref("");
const renameDirName = ref("");

const currentDirName = computed(() => {
  if (!currentPath.value) return "";
  const parts = currentPath.value.split("/").filter(Boolean);
  return parts[parts.length - 1] || "";
});

watch(
  () => currentPath.value,
  () => {
    renameDirName.value = currentDirName.value;
  },
  { immediate: true },
);

function isSafeDirName(name: string): boolean {
  const n = name.trim();
  if (!n) return false;
  if (n === "." || n === "..") return false;
  if (n.includes("/") || n.includes("\\")) return false;
  return true;
}

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

function buildChildPath(parentPath: string, name: string): string {
  return parentPath ? `${parentPath}/${name}` : name;
}

function buildSiblingPath(path: string, name: string): string {
  const parent = parentDirectoryPath(path);
  return parent ? `${parent}/${name}` : name;
}

function parentDirectoryPath(path: string): string {
  const parts = path.split("/").filter(Boolean);
  parts.pop();
  return parts.join("/");
}

function normalizeTargetDirectory(rawPath: string): string {
  return rawPath
    .trim()
    .replace(/\\/g, "/")
    .replace(/^\/+/, "")
    .replace(/\/+$/, "");
}

function resolveMoveTargetPath(file: FileInfo, targetDir: string): string {
  if (
    file.kind === "directory" &&
    (targetDir === file.path || targetDir.startsWith(`${file.path}/`))
  ) {
    throw new Error("不能将目录移动到自身或其子目录");
  }

  const to = buildChildPath(targetDir, file.name);
  if (to === file.path) {
    throw new Error("目标目录未变化");
  }

  return to;
}

function planMoveOperations(
  items: FileInfo[],
  targetDir: string,
  targetEntries: Pick<FileInfo, "path">[] = [],
) {
  const usedTargets = new Set<string>();
  const existingPaths = new Set(targetEntries.map((entry) => entry.path));

  return items.map((file) => {
    const to = resolveMoveTargetPath(file, targetDir);
    if (usedTargets.has(to)) {
      throw new Error(`目标目录中会产生重名项：${file.name}`);
    }
    if (existingPaths.has(to)) {
      throw new Error(`目标目录已存在同名项目：${file.name}`);
    }
    usedTargets.add(to);
    return { file, to };
  });
}

function resetMoveDialogState() {
  showMoveDialog.value = false;
  moveDialogItems.value = [];
  moveDialogInitialPath.value = "";
  moveDialogSubmitting.value = false;
}

function openMoveDialog(items: FileInfo[], initialPath: string) {
  if (items.length === 0) return;
  moveDialogItems.value = items;
  moveDialogInitialPath.value = normalizeTargetDirectory(initialPath);
  showMoveDialog.value = true;
}

function closeMoveDialog() {
  if (moveDialogSubmitting.value) return;
  resetMoveDialogState();
}

function replaceSelectedPath(oldPath: string, newPath: string) {
  if (!selectedPaths.value.has(oldPath)) return;
  const next = new Set(selectedPaths.value);
  next.delete(oldPath);
  next.add(newPath);
  selectedPaths.value = next;
}

async function refreshAfterMutation() {
  await refresh();
  if (searchActive.value) {
    await doSearch(false);
  }
}

async function createDirectoryAt(
  parentPath: string,
  name: string,
): Promise<string> {
  const dirPath = buildChildPath(parentPath, name);
  await filesService.createDirectory(dirPath, `创建目录: ${dirPath}`);
  return dirPath;
}

async function renameEntryPath(
  path: string,
  name: string,
  message: string,
): Promise<string> {
  const targetPath = buildSiblingPath(path, name);
  await filesService.movePath(path, targetPath, message);
  return targetPath;
}

async function promptCreateDirectory(parentPath: string = currentPath.value) {
  const raw = await promptDialog({
    title: "新建目录",
    message: "输入目录名（仅名称，不含路径分隔符）",
    placeholder: "目录名",
  });
  if (raw == null) return;

  const name = normalizeEntryName(raw, "非法目录名");
  if (!name) return;

  try {
    const dirPath = await createDirectoryAt(parentPath, name);
    appStore.success("目录创建成功");
    if (parentPath === currentPath.value) {
      desktopActivePath.value = dirPath;
    } else {
      if (searchActive.value) {
        clearSearch();
      }
      navigateTo(parentPath);
      return;
    }
    await refreshAfterMutation();
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "目录创建失败");
  }
}

async function createSubDir() {
  const name = normalizeEntryName(newDirName.value, "非法目录名");
  if (!name) return;

  dirOpLoading.value = "create";
  try {
    const dirPath = await createDirectoryAt(currentPath.value, name);
    appStore.success("目录创建成功");
    newDirName.value = "";
    desktopActivePath.value = dirPath;
    await refreshAfterMutation();
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "目录创建失败");
  } finally {
    if (dirOpLoading.value === "create") dirOpLoading.value = null;
  }
}

async function renameCurrentDir() {
  if (!currentPath.value) return;
  const name = normalizeEntryName(renameDirName.value, "非法目录名");
  if (!name) return;
  if (name === currentDirName.value) {
    appStore.error("目录名未变化");
    return;
  }

  dirOpLoading.value = "rename";
  const targetPath = buildSiblingPath(currentPath.value, name);
  try {
    const to = await renameEntryPath(
      currentPath.value,
      name,
      `重命名目录: ${currentPath.value} -> ${targetPath}`,
    );
    appStore.success("重命名成功");
    dirManagerOpen.value = false;
    navigateTo(to);
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "重命名失败");
  } finally {
    if (dirOpLoading.value === "rename") dirOpLoading.value = null;
  }
}

async function deleteCurrentDir() {
  if (!currentPath.value) return;

  const expected = currentDirName.value;
  const typed = await promptDialog({
    title: "删除目录",
    message: `危险操作：删除目录 /${currentPath.value}\n\n此操作会删除其下全部内容，并生成提交。\n请输入目录名“${expected}”以确认：`,
    placeholder: expected,
    confirmText: "删除",
    danger: true,
  });
  if (typed == null) return;
  if (typed.trim() !== expected) {
    appStore.error("确认失败：目录名不匹配");
    return;
  }

  const parts = currentPath.value.split("/").filter(Boolean);
  parts.pop();
  const parent = parts.join("/");

  dirOpLoading.value = "delete";
  try {
    await filesService.deleteFile(
      currentPath.value,
      `删除目录: ${currentPath.value}`,
    );
    appStore.success("目录删除成功");
    dirManagerOpen.value = false;
    navigateTo(parent);
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "删除失败");
  } finally {
    if (dirOpLoading.value === "delete") dirOpLoading.value = null;
  }
}

// 4.2: 移动端无限滚动（分批渲染）
const MOBILE_INITIAL_COUNT = 40;
const MOBILE_CHUNK_COUNT = 30;
const mobileVisibleCount = ref(MOBILE_INITIAL_COUNT);
const loadMoreSentinel = ref<HTMLElement | null>(null);
let loadMoreObserver: IntersectionObserver | null = null;

type BrowserListItem = FileInfo & {
  uiRole?: "self" | "parent";
  uiTargetPath?: string;
};

const currentListItem = computed<BrowserListItem>(() => ({
  id: `self:${currentPath.value || "root"}`,
  name: ".",
  path: `__vfiles_shortcut_self__:${currentPath.value || "root"}`,
  kind: "directory",
  created_at: "",
  updated_at: "",
  mime_type: undefined,
  size_bytes: undefined,
  is_text: false,
  uiRole: "self",
  uiTargetPath: currentPath.value || "",
}));

const parentListItem = computed<BrowserListItem>(() => {
  return {
    id: `parent:${currentPath.value || "root"}`,
    name: "..",
    path: `__vfiles_shortcut_parent__:${(parentPath.value ?? currentPath.value) || "root"}`,
    kind: "directory",
    created_at: "",
    updated_at: "",
    mime_type: undefined,
    size_bytes: undefined,
    is_text: false,
    uiRole: "parent",
    uiTargetPath: parentPath.value ?? "",
  };
});

const navigationListItems = computed<BrowserListItem[]>(() => {
  return [
    currentListItem.value,
    parentListItem.value,
    ...sortBrowserItems(files.value, sortState.value),
  ];
});

const sortedSearchResults = computed<FileInfo[]>(() => {
  return sortFiles(searchResults.value, sortState.value);
});

const activeList = computed(() =>
  searchActive.value ? sortedSearchResults.value : navigationListItems.value,
);
const hasMore = computed(
  () => isMobile.value && mobileVisibleCount.value < activeList.value.length,
);
const visibleFiles = computed(() => {
  if (!isMobile.value) return navigationListItems.value;
  return navigationListItems.value.slice(0, mobileVisibleCount.value);
});
const visibleSearchResults = computed(() => {
  if (!isMobile.value) return sortedSearchResults.value;
  return sortedSearchResults.value.slice(0, mobileVisibleCount.value);
});
const desktopItems = computed(() => {
  return searchActive.value
    ? sortedSearchResults.value
    : navigationListItems.value;
});
const desktopActivePath = ref("");

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

    if (
      !desktopItems.value.some((file) => file.path === desktopActivePath.value)
    ) {
      const firstRealItem = desktopItems.value.find(
        (file) => !(file as BrowserListItem).uiRole,
      );
      desktopActivePath.value =
        firstRealItem?.path || desktopItems.value[0].path;
    }
  },
  { immediate: true },
);

function bumpVisibleCount() {
  const total = activeList.value.length;
  mobileVisibleCount.value = Math.min(
    total,
    mobileVisibleCount.value + MOBILE_CHUNK_COUNT,
  );
}

function resetVisibleCount() {
  mobileVisibleCount.value = MOBILE_INITIAL_COUNT;
}

function setupLoadMoreObserver() {
  if (loadMoreObserver) {
    loadMoreObserver.disconnect();
    loadMoreObserver = null;
  }

  if (!isMobile.value) return;
  if (!("IntersectionObserver" in window)) return;
  if (!loadMoreSentinel.value) return;

  loadMoreObserver = new IntersectionObserver(
    (entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      if (!hasMore.value) return;
      bumpVisibleCount();
    },
    { root: null, threshold: 0.1 },
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

onBeforeUnmount(() => {
  if (loadMoreObserver) {
    loadMoreObserver.disconnect();
    loadMoreObserver = null;
  }
});

// 4.2: 下拉刷新 + 手势（边缘右滑返回）
const pullDistance = ref(0);
const pullReady = ref(false);
const pullRefreshing = ref(false);

const pullIndicatorVisible = computed(
  () => pullRefreshing.value || pullDistance.value > 10,
);

const touchStart = ref({ x: 0, y: 0, t: 0 });
const touchMode = ref<"none" | "pull" | "swipe">("none");

const anyModalOpen = computed(() => {
  return (
    showUploader.value ||
    showHistory.value ||
    showShareDialog.value ||
    showMoveDialog.value ||
    preview.value.open
  );
});

function onTouchStart(e: TouchEvent) {
  if (!isMobile.value) return;
  if (anyModalOpen.value) return;
  const t = e.touches[0];
  if (!t) return;
  touchStart.value = { x: t.clientX, y: t.clientY, t: Date.now() };
  touchMode.value = "none";
}

function onTouchMove(e: TouchEvent) {
  if (!isMobile.value) return;
  if (anyModalOpen.value) return;
  const t = e.touches[0];
  if (!t) return;

  const dx = t.clientX - touchStart.value.x;
  const dy = t.clientY - touchStart.value.y;

  if (touchMode.value === "none") {
    if (Math.abs(dx) > 12 && Math.abs(dx) > Math.abs(dy)) {
      touchMode.value = "swipe";
    } else if (dy > 8 && Math.abs(dy) > Math.abs(dx) && window.scrollY <= 0) {
      touchMode.value = "pull";
    }
  }

  if (
    touchMode.value === "pull" &&
    window.scrollY <= 0 &&
    !pullRefreshing.value
  ) {
    const next = Math.min(90, Math.max(0, dy));
    pullDistance.value = next;
    pullReady.value = next >= 60;
  }
}

async function onTouchEnd(e: TouchEvent) {
  if (!isMobile.value) return;
  if (anyModalOpen.value) return;

  const changed = e.changedTouches[0];
  if (!changed) {
    pullDistance.value = 0;
    pullReady.value = false;
    touchMode.value = "none";
    return;
  }

  const dx = changed.clientX - touchStart.value.x;
  const dy = changed.clientY - touchStart.value.y;

  if (touchMode.value === "swipe") {
    const fromEdge = touchStart.value.x <= 24;
    const horizontal = dx > 80 && Math.abs(dy) < 60;
    if (fromEdge && horizontal) {
      goBack();
    }
  }

  if (touchMode.value === "pull" && pullReady.value && !pullRefreshing.value) {
    pullRefreshing.value = true;
    try {
      await Promise.resolve(refresh());
      appStore.success("已刷新");
    } finally {
      pullRefreshing.value = false;
    }
  }

  pullDistance.value = 0;
  pullReady.value = false;
  touchMode.value = "none";
}

function enqueueDownload(kind: DownloadQueueKind, path: string) {
  const wasEmpty = downloadQueue.value.length === 0;
  const filename =
    kind === "folder"
      ? `${path.split("/").filter(Boolean).pop() || "root"}.zip`
      : path.split("/").pop() || "download";

  downloadQueue.value = [
    ...downloadQueue.value,
    {
      id: nextDownloadId++,
      kind,
      path,
      filename,
      status: "queued",
    },
  ];

  // 第一次出现队列时默认展开，方便用户查看进度。
  if (wasEmpty) queueCollapsed.value = false;

  void processQueue();
}

function toggleQueuePanel() {
  queueCollapsed.value = !queueCollapsed.value;
}

async function processQueue() {
  if (downloading.value) return;

  const next = downloadQueue.value.find((x) => x.status === "queued");
  if (!next) return;

  const abort = new AbortController();
  downloadQueue.value = downloadQueue.value.map((x) =>
    x.id === next.id
      ? { ...x, status: "downloading", progress: { loaded: 0 }, abort }
      : x,
  );

  try {
    const onProgress = (p: { loaded: number; total?: number }) => {
      downloadQueue.value = downloadQueue.value.map((x) =>
        x.id === next.id ? { ...x, progress: p } : x,
      );
    };

    const commit = browseCommit.value;
    const result =
      next.kind === "folder"
        ? await filesService.fetchFolderDownload(next.path, commit, {
            signal: abort.signal,
            onProgress,
          })
        : await filesService.fetchFileDownload(next.path, commit, {
            signal: abort.signal,
            onProgress,
          });

    filesService.saveDownloadedBlob(result.blob, result.filename);
    downloadQueue.value = downloadQueue.value.map((x) =>
      x.id === next.id ? { ...x, status: "done", abort: undefined } : x,
    );
  } catch (err: any) {
    const isAbort = err?.name === "AbortError";
    downloadQueue.value = downloadQueue.value.map((x) =>
      x.id === next.id
        ? {
            ...x,
            status: isAbort ? "canceled" : "error",
            error: isAbort
              ? undefined
              : err instanceof Error
                ? err.message
                : "下载失败",
            abort: undefined,
          }
        : x,
    );
  } finally {
    // 继续下一个
    void processQueue();
  }
}

function formatProgress(loaded: number, total: number): string {
  const percent = Math.floor((loaded / total) * 100);
  const formatSize = (bytes: number): string => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
  };
  return ` ${percent}% (${formatSize(loaded)}/${formatSize(total)})`;
}

function cancelItem(id: number) {
  const item = downloadQueue.value.find((x) => x.id === id);
  if (!item) return;

  if (item.status === "queued") {
    downloadQueue.value = downloadQueue.value.map((x) =>
      x.id === id ? { ...x, status: "canceled" } : x,
    );
    return;
  }

  if (item.status === "downloading") {
    item.abort?.abort();
  }
}

function cancelAll() {
  for (const item of downloadQueue.value) {
    if (item.status === "queued") {
      downloadQueue.value = downloadQueue.value.map((x) =>
        x.id === item.id ? { ...x, status: "canceled" } : x,
      );
    } else if (item.status === "downloading") {
      item.abort?.abort();
    }
  }
}

function clearFinished() {
  downloadQueue.value = downloadQueue.value.filter(
    (x) => x.status === "queued" || x.status === "downloading",
  );
}

function removeItem(id: number) {
  downloadQueue.value = downloadQueue.value.filter((x) => x.id !== id);
}

function handleItemClick(file: BrowserListItem) {
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
    navigateTo((file as BrowserListItem).uiTargetPath ?? file.path);
  }
}

function handleDownload(file: FileInfo) {
  // Route downloads through the queue so users get progress, error feedback,
  // and cancellation support instead of a silent browser download.
  enqueueDownload(file.kind === "directory" ? "folder" : "file", file.path);
}

async function handleDelete(file: FileInfo) {
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
    appStore.success(
      file.kind === "directory" ? "目录删除成功" : "文件删除成功",
    );
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "删除失败");
  }
}

async function handleCreateDirectory(file: FileInfo) {
  if (file.kind !== "directory") return;
  await promptCreateDirectory(
    (file as BrowserListItem).uiTargetPath ?? file.path,
  );
}

async function handleRenameEntry(file: FileInfo) {
  const raw = await promptDialog({
    title: "重命名",
    message: "输入新名称（仅名称，不含路径分隔符）",
    defaultValue: file.name,
    placeholder: file.name,
  });
  if (raw == null) return;

  const name = normalizeEntryName(raw, "非法名称");
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
      file.kind === "directory" ? "目录重命名成功" : "重命名成功",
    );
    await refreshAfterMutation();
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "重命名失败");
  }
}

async function handleMoveEntry(file: FileInfo) {
  openMoveDialog([file], parentDirectoryPath(file.path));
}

async function submitMoveDialog(targetDir: string) {
  const items = moveDialogItems.value.slice();
  if (items.length === 0) return;

  const normalizedTargetDir = normalizeTargetDirectory(targetDir);
  moveDialogSubmitting.value = true;

  try {
    const targetEntries = await filesService.getFiles(normalizedTargetDir);
    const operations = planMoveOperations(
      items,
      normalizedTargetDir,
      targetEntries,
    );

    for (const { file, to } of operations) {
      await filesService.movePath(
        file.path,
        to,
        `移动${file.kind === "directory" ? "目录" : "文件"}: ${file.path} -> ${to}`,
      );
      replaceSelectedPath(file.path, to);
      if (desktopActivePath.value === file.path) {
        desktopActivePath.value =
          parentDirectoryPath(to) === currentPath.value ? to : "";
      }
    }

    const successMessage =
      items.length === 1
        ? items[0]?.kind === "directory"
          ? "目录移动成功"
          : "文件移动成功"
        : `已移动 ${items.length} 个项目`;

    if (items.length > 1) {
      clearSelection();
    }

    appStore.success(successMessage);
    resetMoveDialogState();
    await refreshAfterMutation();
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "移动失败");
    moveDialogSubmitting.value = false;
  }
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

async function runSearch() {
  return await doSearch(true);
}

async function doSearch(pushHistoryEnabled: boolean) {
  const q = searchQuery.value.trim();
  searchError.value = null;

  if (!q) {
    clearSearch();
    return;
  }

  searchLoading.value = true;
  searchActive.value = true;

  if (pushHistoryEnabled) {
    pushSearchHistory(q);
  }

  try {
    const scopePath = searchScopeCurrent.value ? filesStore.currentPath : "";
    searchResults.value = await filesService.searchFiles(q, searchMode.value, {
      type: searchType.value,
      path: scopePath,
    });
  } catch (err) {
    searchError.value = err instanceof Error ? err.message : "搜索失败";
    searchResults.value = [];
  } finally {
    searchLoading.value = false;
  }
}

function clearSearch() {
  searchQuery.value = "";
  searchResults.value = [];
  searchError.value = null;
  searchActive.value = false;
}

function toggleBatchMode() {
  batchMode.value = !batchMode.value;
  if (!batchMode.value) {
    clearSelection();
  }
}

defineExpose({
  openUploader: () => {
    showUploader.value = true;
  },
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
    showUploader.value ||
    showHistory.value ||
    showShareDialog.value ||
    showMoveDialog.value ||
    dirManagerOpen.value
  );
}

/** 当前键盘操作的目标：优先高亮行，其次唯一的已选条目。 */
function findActiveItem(): BrowserListItem | undefined {
  const list = (
    searchActive.value ? sortedSearchResults.value : navigationListItems.value
  ) as BrowserListItem[];

  if (desktopActivePath.value) {
    const active = list.find((file) => file.path === desktopActivePath.value);
    if (active && !active.uiRole) return active;
  }

  if (selectedPaths.value.size === 1) {
    const [only] = selectedPaths.value;
    const selected = list.find((file) => file.path === only);
    if (selected && !selected.uiRole) return selected;
  }

  return undefined;
}

function toggleSelect(file: FileInfo) {
  desktopActivePath.value = file.path;
  lastSelectedPath.value = file.path;
  const next = new Set(selectedPaths.value);
  if (next.has(file.path)) {
    next.delete(file.path);
  } else {
    next.add(file.path);
  }
  selectedPaths.value = next;
}

/** 当前可见的、可选择的真实条目（排除 `.`/`..` 快捷项）。 */
function selectableItems(): BrowserListItem[] {
  const list = searchActive.value
    ? sortedSearchResults.value
    : navigationListItems.value;
  return list.filter((file) => !(file as BrowserListItem).uiRole);
}

/**
 * Shift/Ctrl(⌘) 点击：Shift 选中最近一次点击到当前项的连续区间，
 * Ctrl(⌘) 切换单项选择；两者都会自动进入批量模式。
 */
function handleModifierSelect(payload: {
  file: FileInfo;
  shift: boolean;
  meta: boolean;
}) {
  const file = payload.file as BrowserListItem;
  if (file.uiRole) return;

  if (payload.shift && lastSelectedPath.value) {
    const list = selectableItems();
    const from = list.findIndex((item) => item.path === lastSelectedPath.value);
    const to = list.findIndex((item) => item.path === file.path);
    if (from !== -1 && to !== -1) {
      const [start, end] = from <= to ? [from, to] : [to, from];
      const next = new Set(selectedPaths.value);
      for (let index = start; index <= end; index += 1) {
        next.add(list[index].path);
      }
      selectedPaths.value = next;
      batchMode.value = true;
      desktopActivePath.value = file.path;
      return;
    }
  }

  batchMode.value = true;
  toggleSelect(file);
}

function handleContextMenu(payload: { file: FileInfo; x: number; y: number }) {
  const file = payload.file as BrowserListItem;
  if (file.uiRole) return;

  if (batchMode.value && !selectedPaths.value.has(file.path)) {
    selectedPaths.value = new Set([file.path]);
    lastSelectedPath.value = file.path;
  }
  desktopActivePath.value = file.path;
  contextMenu.value = {
    show: true,
    x: payload.x,
    y: payload.y,
    file: payload.file,
  };
}

function handleContextMenuSelect(key: string) {
  const file = contextMenu.value.file;
  if (!file) return;

  switch (key) {
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

function toggleSelectAll() {
  if (!batchMode.value) return;
  const selectable = (
    searchActive.value ? sortedSearchResults.value : navigationListItems.value
  ).filter((file) => !(file as BrowserListItem).uiRole);

  const allSelected =
    selectable.length > 0 &&
    selectable.every((file) => selectedPaths.value.has(file.path));

  if (allSelected) {
    clearSelection();
  } else {
    selectAllVisible();
  }
}

function clearSelection() {
  selectedPaths.value = new Set();
}

function selectAllVisible() {
  const list = searchActive.value
    ? searchResults.value
    : navigationListItems.value.filter(
        (file) => !(file as BrowserListItem).uiRole,
      );
  const next = new Set(selectedPaths.value);
  for (const f of list) {
    next.add(f.path);
  }
  selectedPaths.value = next;
}

function getSelectedItems(): FileInfo[] {
  const list = searchActive.value ? searchResults.value : files.value;
  const map = new Map(list.map((f) => [f.path, f] as const));
  const items: FileInfo[] = [];
  for (const p of selectedPaths.value) {
    const it = map.get(p);
    if (it) items.push(it);
  }
  return items;
}

async function batchDownload() {
  const items = getSelectedItems();
  if (items.length === 0) return;

  // 分离文件和文件夹
  const files = items.filter((f) => f.kind !== "directory");
  const folders = items.filter((f) => f.kind === "directory");

  const totalCount = files.length + folders.length;
  if (totalCount > 10) {
    const ok = await confirmDialog({
      title: "批量下载",
      message: `将开始下载 ${totalCount} 个项目，可能会被浏览器拦截弹窗。继续吗？`,
      confirmText: "继续下载",
    });
    if (!ok) return;
  }

  // 单文件使用浏览器原生下载
  for (const f of files) {
    filesService.downloadFile(f.path, browseCommit.value);
  }

  // 文件夹也使用浏览器原生下载
  for (const f of folders) {
    filesService.downloadFolder(f.path, browseCommit.value);
  }

  appStore.success(`已开始下载 ${totalCount} 个项目`);
}

async function batchDelete() {
  const items = getSelectedItems();
  if (items.length === 0) return;

  const ok = await confirmDialog({
    title: "批量删除",
    message: `确定要删除 ${items.length} 项吗？此操作会生成一次或多次提交。`,
    confirmText: "删除",
    danger: true,
  });
  if (!ok) return;

  try {
    for (const f of items) {
      await filesService.deleteFile(f.path, "批量删除");
    }
    appStore.success("批量删除完成");
    clearSelection();
    await refresh();
    if (searchActive.value) {
      await doSearch(false);
    }
  } catch (err) {
    appStore.error(err instanceof Error ? err.message : "批量删除失败");
  }
}

async function batchMove() {
  const items = getSelectedItems();
  if (items.length === 0) return;

  openMoveDialog(
    items,
    filesStore.currentPath || parentDirectoryPath(items[0]?.path || ""),
  );
}

async function renameSelected() {
  const items = getSelectedItems();
  if (items.length !== 1) return;
  await handleRenameEntry(items[0]);
}
</script>

<style scoped>
.file-browser {
  --explorer-accent: #2f6db6;
  --explorer-accent-soft: rgba(47, 109, 182, 0.1);
  --explorer-panel-bg: rgba(255, 255, 255, 0.92);
  --explorer-panel-border: #d6dfeb;
  --explorer-shell-bg: linear-gradient(180deg, #f7f9fc 0%, #eef3f8 100%);
  --explorer-list-bg: rgba(255, 255, 255, 0.9);
  margin: 0 auto;
  padding: 0;
}

.file-browser-box {
  display: flex;
  flex-direction: column;
  border-radius: 22px;
  border: 1px solid var(--explorer-panel-border);
  background: var(--explorer-shell-bg);
  box-shadow: 0 20px 48px rgba(32, 52, 88, 0.12);
  padding: 1rem;
}

.breadcrumb-bar {
  margin-bottom: 1rem;
  padding: 0.55rem 0.7rem;
  border-radius: 14px;
  background: rgba(255, 255, 255, 0.65);
  border: 1px solid rgba(214, 223, 235, 0.9);
}

.breadcrumb-current-path {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.95rem;
  font-weight: 600;
  line-height: 1.35;
  color: #24384d;
}

.file-browser-toolbar {
  position: relative;
  z-index: 2;
  margin-bottom: 0.85rem;
}

.desktop-command-bar {
  display: block;
  margin-bottom: 0.65rem;
}

.desktop-command-group {
  min-width: 0;
  display: flex;
  gap: 0.55rem;
  padding: 0.72rem;
  border-radius: 14px;
  border: 1px solid var(--explorer-panel-border);
  background: var(--explorer-panel-bg);
}

.desktop-command-group {
  flex-wrap: wrap;
  align-items: center;
}

.desktop-search-box {
  position: relative;
  z-index: 3;
  margin-left: auto;
  flex: 1 1 auto;
  min-width: 0;
  max-width: none;
}

.desktop-search-inline {
  display: flex;
  align-items: stretch;
  gap: 0.45rem;
}

.desktop-search-field {
  flex: 1 1 auto;
  min-width: 0;
}

.desktop-command-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.38rem;
  min-height: 2rem;
  border-radius: 999px;
  padding-inline: 0.82rem;
  font-weight: 600;
}

.desktop-search-panel {
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
  padding: 0.72rem;
  border-radius: 14px;
  border: 1px solid var(--explorer-panel-border);
  background: var(--explorer-panel-bg);
}

.desktop-search-panel--dropdown {
  position: absolute;
  top: calc(100% + 0.45rem);
  left: auto;
  right: 0;
  width: max-content;
  max-width: min(calc(100vw - 2rem), 28rem);
  z-index: 25;
  background: #ffffff;
  opacity: 1;
  isolation: isolate;
  box-shadow: 0 22px 42px rgba(24, 38, 60, 0.18);
}

.desktop-search-panel--dropdown::after {
  content: "";
  position: absolute;
  right: 1.25rem;
  top: -0.42rem;
  width: 0.82rem;
  height: 0.82rem;
  background: #ffffff;
  border-left: 1px solid var(--explorer-panel-border);
  border-top: 1px solid var(--explorer-panel-border);
  transform: rotate(45deg);
}

.desktop-search-panel-heading {
  font-size: 0.7rem;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: #6f8299;
}

.desktop-search-control {
  min-height: 2rem;
  border-radius: 999px;
}

.desktop-search-action-group {
  display: inline-flex;
  align-items: stretch;
  flex-shrink: 0;
}

.desktop-search-button {
  justify-content: center;
  border-top-right-radius: 0;
  border-bottom-right-radius: 0;
}

.desktop-search-toggle {
  padding-inline: 0.68rem;
  border-top-left-radius: 0;
  border-bottom-left-radius: 0;
}

.desktop-search-toggle svg {
  transition: transform 0.18s ease;
}

.desktop-search-toggle.is-open svg {
  transform: rotate(180deg);
}

.desktop-search-filters {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  flex-wrap: wrap;
  color: #5d6d81;
  font-size: 0.8rem;
}

.desktop-filter-pill {
  display: inline-flex;
  align-items: center;
  gap: 0.42rem;
  padding: 0.38rem 0.72rem;
  border-radius: 999px;
  border: 1px solid #d6dfeb;
  background: #f5f8fc;
  line-height: 1;
}

.desktop-filter-pill input {
  margin: 0;
}

.desktop-filter-select select {
  border-radius: 999px;
  background-color: #ffffff;
}

.desktop-search-dropdown-actions {
  display: flex;
  justify-content: flex-end;
}

.desktop-search-clear {
  min-width: 0;
}

.desktop-batch-strip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.58rem 0.8rem;
  border-radius: 12px;
  background: rgba(47, 109, 182, 0.08);
  border: 1px solid rgba(47, 109, 182, 0.16);
  margin-bottom: 0.65rem;
}

.desktop-list-primary-shell {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  position: relative;
  z-index: 0;
}

.desktop-batch-meta {
  color: #2b4d75;
  font-weight: 700;
}

.desktop-batch-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 0.4rem;
}

.desktop-batch-actions .button {
  border-radius: 999px;
  font-weight: 600;
}

.desktop-list-shell {
  min-width: 0;
  border-radius: 18px;
  border: 1px solid var(--explorer-panel-border);
  background: var(--explorer-panel-bg);
}

.desktop-content-pane {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.desktop-list-shell {
  display: flex;
  flex-direction: column;
  padding: 0.8rem;
}

.desktop-pane-section + .desktop-pane-section {
  margin-top: 1rem;
}

.desktop-pane-heading {
  margin: 0 0 0.65rem;
  font-size: 0.74rem;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: #607287;
}

.desktop-nav-item {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: flex-start;
  padding: 0.7rem 0.8rem;
  border: none;
  border-radius: 12px;
  background: transparent;
  color: #314255;
  cursor: pointer;
  transition:
    background-color 0.16s ease,
    color 0.16s ease;
  text-align: left;
}

.desktop-nav-item:hover:not(:disabled),
.desktop-nav-item.is-active {
  background: var(--explorer-accent-soft);
  color: #184d9b;
}

.desktop-nav-item.is-empty,
.desktop-nav-item:disabled {
  color: #98a5b5;
  cursor: default;
}

.desktop-list-meta {
  margin-bottom: 0.7rem;
  color: #607287;
  font-size: 0.78rem;
}

.desktop-status-bar {
  display: flex;
  align-items: center;
  gap: 1rem;
  flex-wrap: wrap;
  padding: 0.65rem 0.85rem;
  border-radius: 14px;
  border: 1px solid var(--explorer-panel-border);
  background: rgba(248, 250, 253, 0.92);
  color: #627386;
  font-size: 0.78rem;
}

.desktop-status-shortcuts {
  margin-left: auto;
  color: #90a0b2;
  white-space: nowrap;
}

.desktop-detail-card {
  display: flex;
  flex-direction: column;
  gap: 0.8rem;
}

.desktop-detail-card.is-empty {
  min-height: 220px;
  justify-content: center;
}

.desktop-detail-name {
  margin: 0;
  font-size: 1.15rem;
  line-height: 1.35;
  color: #223448;
  word-break: break-word;
}

.desktop-detail-path {
  margin: -0.2rem 0 0;
  color: #7a8a9e;
  font-size: 0.82rem;
  word-break: break-all;
}

.desktop-detail-tags {
  display: flex;
  gap: 0.45rem;
  flex-wrap: wrap;
}

.desktop-detail-grid {
  display: grid;
  gap: 0.75rem;
  margin: 0;
}

.desktop-detail-grid dt {
  margin: 0 0 0.2rem;
  font-size: 0.74rem;
  letter-spacing: 0.03em;
  text-transform: uppercase;
  color: #75859a;
}

.desktop-detail-grid dd {
  margin: 0;
  color: #314255;
  font-size: 0.9rem;
  word-break: break-word;
}

.desktop-detail-actions {
  display: grid;
  gap: 0.55rem;
}

.desktop-detail-actions .button {
  justify-content: flex-start;
}

.spinner {
  width: 40px;
  height: 40px;
  border: 3px solid rgba(0, 0, 0, 0.1);
  border-top-color: #3273dc;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  margin: 0 auto;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.preview-text {
  max-height: 60vh;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-word;
}

.preview-frame {
  height: 70vh;
}

.preview-iframe {
  width: 100%;
  height: 100%;
  border: 0;
}

.preview-media {
  max-height: 70vh;
}

.preview-video {
  width: 100%;
  max-height: 70vh;
}

.preview-audio {
  width: 100%;
}

.markdown-body :deep(pre) {
  max-height: 60vh;
  overflow: auto;
}

.preview-code {
  max-height: 60vh;
  overflow: auto;
  white-space: pre;
}

.hljs :deep(.hljs-comment),
.hljs :deep(.hljs-quote) {
  opacity: 0.7;
}

.hljs :deep(.hljs-keyword),
.hljs :deep(.hljs-selector-tag),
.hljs :deep(.hljs-title) {
  font-weight: 600;
}

.hljs :deep(.hljs-string) {
  font-style: italic;
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
    display: none;
  }

  .breadcrumb-actions .buttons {
    flex-wrap: nowrap;
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

@media screen and (max-width: 1023px) {
  .desktop-command-bar {
    grid-template-columns: 1fr;
  }
}
</style>
