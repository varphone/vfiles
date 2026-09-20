<template>
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
                      :ref="setDesktopSearchInput"
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
      />

      <template v-if="!isMobile">
        <div class="desktop-list-primary-shell">
          <div class="desktop-list-shell">
            <FileSkeleton
              v-if="loading"
              :variant="viewMode === 'grid' ? 'grid' : 'list'"
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
                class="desktop-load-more has-text-centered has-text-grey is-size-7 py-3"
              >
                继续下滑加载更多（已显示 {{ desktopItems.length }} /
                {{ activeList.length }}）...
              </div>
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
        <FileSkeleton
          v-if="loading"
          :variant="viewMode === 'grid' ? 'grid' : 'list'"
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

      <div v-if="previewTotal > 1" class="preview-nav">
        <button
          class="button is-small is-light"
          :disabled="!canGoPrev"
          title="上一张（←）"
          @click="prevPreview"
        >
          <IconChevronLeft :size="16" />
          <span>上一张</span>
        </button>
        <span class="preview-position">
          {{ Math.max(previewIndex + 1, 1) }} / {{ previewTotal }}
        </span>
        <button
          class="button is-small is-light"
          :disabled="!canGoNext"
          title="下一张（→）"
          @click="nextPreview"
        >
          <span>下一张</span>
          <IconChevronRight :size="16" />
        </button>
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
  IconChevronLeft,
  IconChevronRight,
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
import FileList from "./FileList.vue";
import FileGrid from "./FileGrid.vue";
import ViewOptions from "./ViewOptions.vue";
import Breadcrumb from "./Breadcrumb.vue";
import FileSkeleton from "./FileSkeleton.vue";
import DownloadQueuePanel from "./DownloadQueuePanel.vue";
import ContextMenu, { type ContextMenuItem } from "./ContextMenu.vue";
import MoveDialog from "./MoveDialog.vue";
import FileUploader from "../file-uploader/FileUploader.vue";
import VersionHistory from "../version-history/VersionHistory.vue";
import Modal from "../common/Modal.vue";
import ShareDialog from "../common/ShareDialog.vue";
import { promptDialog } from "../../composables/dialog";
import { useDownloadQueue } from "../../composables/useDownloadQueue";
import { useFilePreview } from "../../composables/useFilePreview";
import { useFileSearch } from "../../composables/useFileSearch";
import { useDirectoryManager } from "../../composables/useDirectoryManager";
import { useFileSelection } from "../../composables/useFileSelection";
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

const MOBILE_LAYOUT_MEDIA_QUERY = "(max-width: 1023px)";

function updateIsMobile() {
  isMobile.value = window.matchMedia(MOBILE_LAYOUT_MEDIA_QUERY).matches;
}

const showUploader = ref(false);
const showHistory = ref(false);
const showShareDialog = ref(false);
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

/** 预览导航使用的文件列表，跟随当前视图（目录或搜索结果）。 */
const previewableFiles = computed<FileInfo[]>(() =>
  (searchActive.value
    ? sortedSearchResults.value
    : navigationListItems.value
  ).filter((file) => file.kind === "file" && !(file as BrowserListItem).uiRole),
);

const {
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
  closeDesktopSearch,
  toggleDesktopSearch,
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
  items.push({ key: "rename", label: "重命名", icon: IconPencil });
  items.push({ key: "move", label: "移动", icon: IconArrowsDiff });
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
const hasMore = computed(() => visibleCount.value < activeList.value.length);
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

  const before = visibleCount.value;
  bumpVisibleCount();
  if (visibleCount.value === before) return;
  void nextTick().then(() => maybeLoadMore());
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

async function handleDropOnFolder(targetDir: string) {
  const file = draggingFile.value;
  draggingFile.value = null;
  if (!file) return;
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

function handleContextMenu(payload: { file: FileInfo; x: number; y: number }) {
  const file = payload.file as BrowserListItem;
  if (file.uiRole) return;

  narrowSelectionTo(file.path);
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

.preview-nav {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.75rem;
  margin-top: 0.85rem;
  padding-top: 0.75rem;
  border-top: 1px solid #eef1f5;
}

.preview-position {
  min-width: 4.5rem;
  text-align: center;
  font-size: 0.8rem;
  color: #627386;
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
