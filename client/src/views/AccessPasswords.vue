<template>
  <SettingsLayout
    title="访问密码"
    description="为同一账号创建多条受限登录密码：主密码拥有完整权限，访问密码只能访问指定目录。"
    active="passwords"
  >
    <template #actions>
      <div class="passwords-header-actions">
          <button
            class="vf-ghost-button is-primary"
            type="button"
            @click="openCreate"
          >
            <IconPlus :size="16" />
            <span>新建密码</span>
          </button>
          <button
            class="vf-ghost-button"
            type="button"
            :disabled="loading"
            @click="reload"
          >
            <IconRefresh :size="16" />
            <span>刷新</span>
          </button>
        </div>
    </template>

    <div class="settings-content">
      <section class="settings-table-panel" aria-label="访问密码列表">
      <p class="passwords-note">
        <IconShieldLock :size="14" />
        <span>
          修改或删除访问密码会立即撤销使用该密码的已有会话。密码只在创建或轮换时提交，列表不会显示明文。
        </span>
      </p>

      <EmptyState
        v-if="error"
        :icon="IconAlertCircle"
        tone="error"
        title="加载失败"
        :hint="error"
      >
        <template #actions>
          <button class="vf-ghost-button is-primary" @click="reload">
            <IconRefresh :size="16" />
            <span>重试</span>
          </button>
        </template>
      </EmptyState>

      <SkeletonList
        v-else-if="loading && credentials.length === 0"
        class="settings-table-state"
        :rows="3"
        label="加载访问密码"
      />

      <EmptyState
        v-else-if="credentials.length === 0"
        :icon="IconKey"
        title="还没有访问密码"
        hint="新建一条受限密码，把指定目录以只读或可写方式交给协作者"
      >
        <template #actions>
          <button class="vf-ghost-button is-primary" @click="openCreate">
            <IconPlus :size="16" />
            <span>新建密码</span>
          </button>
        </template>
      </EmptyState>

      <div v-else-if="isCompact" class="passwords-cards">
        <article
          v-for="credential in credentials"
          :key="credential.id"
          class="passwords-card"
        >
          <header class="passwords-card-head">
            <span class="passwords-name" :title="credential.label">
              {{ credential.label }}
            </span>
            <Badge
              class="password-mode"
              :tone="credential.canWrite ? 'primary' : 'neutral'"
              size="sm"
            >
              {{ credential.canWrite ? "可写" : "只读" }}
            </Badge>
          </header>

          <dl class="passwords-card-meta">
            <div class="passwords-card-roots">
              <dt>允许目录</dt>
              <dd>
                <div class="passwords-roots">
                  <span
                    v-for="root in credential.roots"
                    :key="root"
                    class="password-root-chip"
                    :title="`/${root}`"
                  >
                    {{ root }}
                  </span>
                </div>
              </dd>
            </div>
            <div>
              <dt>创建时间</dt>
              <dd>{{ formatRelativeDate(credential.createdAt) }}</dd>
            </div>
            <div>
              <dt>最近使用</dt>
              <dd>
                {{
                  credential.lastUsedAt
                    ? formatRelativeDate(credential.lastUsedAt)
                    : "从未使用"
                }}
              </dd>
            </div>
          </dl>

          <div class="passwords-card-actions">
            <button
              class="vf-ghost-button passwords-action"
              type="button"
              @click="openEdit(credential)"
            >
              <IconPencil :size="14" />
              <span>编辑</span>
            </button>
            <button
              class="vf-ghost-button passwords-action passwords-delete"
              type="button"
              @click="remove(credential)"
            >
              <IconTrash :size="14" />
              <span>删除</span>
            </button>
          </div>
        </article>
      </div>

      <div v-else class="passwords-table-wrap">
        <table class="passwords-table">
          <thead>
            <tr>
              <th scope="col" class="passwords-col-name">备注</th>
              <th scope="col" class="passwords-col-roots">允许目录</th>
              <th scope="col" class="passwords-col-mode">权限</th>
              <th scope="col" class="passwords-col-created">创建时间</th>
              <th scope="col" class="passwords-col-used">最近使用</th>
              <th scope="col" class="passwords-col-actions">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="credential in credentials"
              :key="credential.id"
              class="passwords-row"
            >
              <td class="passwords-name" :title="credential.label">
                {{ credential.label }}
              </td>
              <td>
                <div class="passwords-roots">
                  <span
                    v-for="root in credential.roots"
                    :key="root"
                    class="password-root-chip"
                    :title="`/${root}`"
                  >
                    {{ root }}
                  </span>
                </div>
              </td>
              <td>
                <Badge
                  class="password-mode"
                  :tone="credential.canWrite ? 'primary' : 'neutral'"
                  size="sm"
                >
                  {{ credential.canWrite ? "可写" : "只读" }}
                </Badge>
              </td>
              <td class="passwords-muted">
                {{ formatRelativeDate(credential.createdAt) }}
              </td>
              <td class="passwords-muted">
                {{
                  credential.lastUsedAt
                    ? formatRelativeDate(credential.lastUsedAt)
                    : "从未使用"
                }}
              </td>
              <td class="passwords-actions">
                <div class="passwords-actions-inner">
                  <button
                    class="vf-ghost-button passwords-action"
                    type="button"
                    @click="openEdit(credential)"
                  >
                    <IconPencil :size="14" />
                    <span>编辑</span>
                  </button>
                  <button
                    class="vf-ghost-button passwords-action passwords-delete"
                    type="button"
                    @click="remove(credential)"
                  >
                    <IconTrash :size="14" />
                    <span>删除</span>
                  </button>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      </section>
    </div>

    <!-- 新建 / 编辑 -->
    <Modal
      :show="formOpen"
      :title="editingId ? '编辑访问密码' : '新建访问密码'"
      @close="closeForm"
    >
      <div class="passwords-form">
        <label class="passwords-label" for="password-label">备注</label>
        <input
          id="password-label"
          v-model.trim="form.label"
          class="input is-small"
          type="text"
          maxlength="64"
          autocomplete="off"
          placeholder="例如：给设计团队的只读账号"
          :aria-invalid="formError ? 'true' : undefined"
          :aria-describedby="formError ? 'password-form-error' : undefined"
        />

        <label class="passwords-label" for="password-secret">密码</label>
        <input
          id="password-secret"
          v-model="form.password"
          class="input is-small"
          type="password"
          autocomplete="new-password"
          :placeholder="editingId ? '留空表示不修改' : '至少 6 位'"
          :aria-invalid="formError ? 'true' : undefined"
          :aria-describedby="formError ? 'password-form-error' : undefined"
        />

        <label class="passwords-label">允许目录</label>
        <div class="passwords-roots-editor">
          <span
            v-for="root in form.roots"
            :key="root"
            class="password-root-chip is-editable"
          >
            <span>{{ root }}</span>
            <button
              class="password-root-remove"
              type="button"
              :aria-label="`移除目录 ${root}`"
              title="移除"
              @click="removeRoot(root)"
            >
              <IconX :size="12" />
            </button>
          </span>
          <button
            class="vf-ghost-button passwords-add-root"
            type="button"
            :disabled="form.roots.length >= MAX_ROOTS"
            @click="openDirectoryPicker"
          >
            <IconPlus :size="14" />
            <span>添加目录</span>
          </button>
        </div>
        <p class="passwords-hint">
          至少 1 个、最多 {{ MAX_ROOTS }} 个目录；不能选择命名空间根目录。
        </p>

        <label class="passwords-write-toggle">
          <input v-model="form.canWrite" type="checkbox" />
          <span>允许写入（可上传、修改、移动、删除）</span>
        </label>

        <p
          v-if="formError"
          id="password-form-error"
          class="passwords-error"
          role="alert"
        >
          {{ formError }}
        </p>
      </div>

      <template #footer>
        <button class="vf-ghost-button" type="button" @click="closeForm">
          取消
        </button>
        <button
          class="vf-ghost-button is-primary"
          type="button"
          :disabled="submitting"
          @click="submitForm"
        >
          <IconCheck :size="16" />
          <span>{{ submitting ? "保存中…" : "保存" }}</span>
        </button>
      </template>
    </Modal>

    <!-- 目录选择器：只列出真实子目录，可进入、可选中当前目录 -->
    <Modal
      :show="pickerOpen"
      title="选择允许访问的目录"
      @close="closeDirectoryPicker"
    >
      <div class="directory-picker">
        <div class="directory-picker-path">
          <button
            class="vf-ghost-button"
            type="button"
            :disabled="pickerPath === '' || pickerLoading"
            @click="pickerGoUp"
          >
            <IconArrowLeft :size="14" />
            <span>上一级</span>
          </button>
          <code class="directory-picker-current">/{{ pickerPath }}</code>
        </div>

        <p v-if="pickerError" class="passwords-error" role="alert">
          {{ pickerError }}
        </p>
        <SkeletonList
          v-else-if="pickerLoading"
          :rows="3"
          label="加载目录"
        />
        <EmptyState
          v-else-if="pickerDirectories.length === 0"
          :icon="IconFolder"
          compact
          title="没有子目录"
          hint="可以返回上一级，或选择当前目录"
        />
        <ul v-else class="directory-picker-list">
          <li
            v-for="directory in pickerDirectories"
            :key="directory.path"
            class="directory-picker-row"
          >
            <button
              class="directory-picker-enter"
              type="button"
              :title="`进入 /${directory.path}`"
              @click="enterPickerDirectory(directory.path)"
            >
              <IconFolder :size="16" />
              <span>{{ directory.name }}</span>
            </button>
            <button
              class="vf-ghost-button is-small"
              type="button"
              :disabled="!canPickPath(directory.path)"
              @click="selectPickerPath(directory.path)"
            >
              选择
            </button>
          </li>
        </ul>

        <div class="directory-picker-footer">
          <span class="passwords-hint">
            {{
              pickerPath
                ? `当前：${pickerPath}`
                : "当前：命名空间根目录（不可选）"
            }}
          </span>
          <button
            class="vf-ghost-button is-primary"
            type="button"
            :disabled="!canPickPath(pickerPath)"
            @click="selectPickerPath(pickerPath)"
          >
            选择当前目录
          </button>
        </div>
      </div>
    </Modal>
  </SettingsLayout>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  IconAlertCircle,
  IconArrowLeft,
  IconCheck,
  IconFolder,
  IconKey,
  IconPencil,
  IconPlus,
  IconRefresh,
  IconShieldLock,
  IconTrash,
  IconX,
} from "@tabler/icons-vue";
import EmptyState from "../components/common/EmptyState.vue";
import SettingsLayout from "../layouts/SettingsLayout.vue";
import Modal from "../components/common/Modal.vue";
import SkeletonList from "../components/common/SkeletonList.vue";
import Badge from "../ui/Badge.vue";
import { confirmDialog } from "../composables/dialog";
import { useMediaQuery } from "../composables/useMediaQuery";
import { authService } from "../services/auth.service";
import { filesService } from "../services/files.service";
import { useAppStore } from "../stores/app.store";
import { formatRelativeDate } from "../utils/filePresentation";
import {
  normalizeScopePath,
  normalizeScopeRoots,
} from "../utils/sessionScope";
import type { FileInfo } from "../types";
import type { LoginCredential } from "../services/auth.service";

const MAX_ROOTS = 8;
const MIN_PASSWORD_LENGTH = 6;

const app = useAppStore();
const isCompact = useMediaQuery("(max-width: 768px)");

const credentials = ref<LoginCredential[]>([]);
const loading = ref(false);
const error = ref("");

const formOpen = ref(false);
const submitting = ref(false);
const formError = ref("");
const editingId = ref("");
const form = ref({
  label: "",
  password: "",
  roots: [] as string[],
  canWrite: false,
});

const pickerOpen = ref(false);
const pickerPath = ref("");
const pickerDirectories = ref<FileInfo[]>([]);
const pickerLoading = ref(false);
const pickerError = ref("");
let pickerRequestSeq = 0;

const isEditing = computed(() => editingId.value !== "");

async function reload(): Promise<void> {
  loading.value = true;
  error.value = "";
  try {
    credentials.value = await authService.listLoginCredentials();
  } catch (e) {
    error.value = e instanceof Error ? e.message : "加载访问密码失败";
  } finally {
    loading.value = false;
  }
}


function resetForm(): void {
  form.value = { label: "", password: "", roots: [], canWrite: false };
  formError.value = "";
  editingId.value = "";
}

function openCreate(): void {
  resetForm();
  formOpen.value = true;
}

function openEdit(credential: LoginCredential): void {
  editingId.value = credential.id;
  form.value = {
    label: credential.label,
    password: "",
    roots: normalizeScopeRoots(credential.roots),
    canWrite: credential.canWrite,
  };
  formError.value = "";
  formOpen.value = true;
}

function closeForm(): void {
  formOpen.value = false;
  submitting.value = false;
}

function removeRoot(root: string): void {
  form.value.roots = form.value.roots.filter((value) => value !== root);
}

function validateForm(): string {
  const label = form.value.label.trim();
  if (!label) return "请输入备注";
  if (Array.from(label).length > 64) return "备注长度需在 1-64 个字符之间";

  const password = form.value.password;
  if (!isEditing.value && !password) return "请输入密码";
  if (password) {
    const length = Array.from(password).length;
    if (length < MIN_PASSWORD_LENGTH || length > 256) {
      return "密码长度需在 6-256 个字符之间";
    }
  }

  const roots = normalizeScopeRoots(form.value.roots);
  if (roots.length === 0) return "至少选择一个允许访问的目录";
  if (roots.length > MAX_ROOTS) return `最多选择 ${MAX_ROOTS} 个允许访问的目录`;
  return "";
}

async function submitForm(): Promise<void> {
  if (submitting.value) return;
  const validation = validateForm();
  if (validation) {
    formError.value = validation;
    return;
  }

  submitting.value = true;
  formError.value = "";
  try {
    const label = form.value.label.trim();
    const roots = normalizeScopeRoots(form.value.roots);
    if (isEditing.value) {
      await authService.updateLoginCredential(editingId.value, {
        label,
        roots,
        canWrite: form.value.canWrite,
        ...(form.value.password ? { password: form.value.password } : {}),
      });
      app.success(`已更新访问密码「${label}」`);
    } else {
      await authService.createLoginCredential({
        label,
        password: form.value.password,
        roots,
        canWrite: form.value.canWrite,
      });
      app.success(`已创建访问密码「${label}」`);
    }
    closeForm();
    await reload();
  } catch (e) {
    // 403 / 后端校验错误：直接展示服务端 message
    formError.value = e instanceof Error ? e.message : "保存访问密码失败";
  } finally {
    submitting.value = false;
  }
}

async function remove(credential: LoginCredential): Promise<void> {
  const ok = await confirmDialog({
    title: "删除访问密码",
    message: `确定删除「${credential.label}」吗？\n使用该密码的已有会话会立即失效。`,
    confirmText: "删除",
    danger: true,
  });
  if (!ok) return;

  try {
    await authService.deleteLoginCredential(credential.id);
    app.success(`已删除访问密码「${credential.label}」`);
    await reload();
  } catch (e) {
    app.error(e instanceof Error ? e.message : "删除访问密码失败");
  }
}

function openDirectoryPicker(): void {
  if (form.value.roots.length >= MAX_ROOTS) return;
  pickerPath.value = "";
  pickerDirectories.value = [];
  pickerError.value = "";
  pickerOpen.value = true;
  void loadPickerDirectories();
}

function closeDirectoryPicker(): void {
  pickerOpen.value = false;
  pickerRequestSeq += 1;
  pickerLoading.value = false;
}

function pickerParent(path: string): string {
  const normalized = normalizeScopePath(path);
  if (!normalized.includes("/")) return "";
  return normalized.slice(0, normalized.lastIndexOf("/"));
}

function pickerGoUp(): void {
  if (!pickerPath.value) return;
  pickerPath.value = pickerParent(pickerPath.value);
  void loadPickerDirectories();
}

function enterPickerDirectory(path: string): void {
  pickerPath.value = normalizeScopePath(path);
  void loadPickerDirectories();
}

async function loadPickerDirectories(): Promise<void> {
  pickerRequestSeq += 1;
  const seq = pickerRequestSeq;
  pickerLoading.value = true;
  pickerError.value = "";
  try {
    const page = await filesService.getDirectoriesPage(pickerPath.value, {
      limit: 200,
      offset: 0,
    });
    if (seq !== pickerRequestSeq) return;
    pickerDirectories.value = page.items.map((item) => ({
      ...item,
      path: normalizeScopePath(
        item.path ||
          (pickerPath.value
            ? `${pickerPath.value}/${item.name}`
            : item.name),
      ),
    }));
  } catch (e) {
    if (seq !== pickerRequestSeq) return;
    pickerDirectories.value = [];
    pickerError.value = e instanceof Error ? e.message : "加载目录失败";
  } finally {
    if (seq === pickerRequestSeq) pickerLoading.value = false;
  }
}

function canPickPath(path: string): boolean {
  const normalized = normalizeScopePath(path);
  if (!normalized) return false;
  if (form.value.roots.length >= MAX_ROOTS) return false;
  return !form.value.roots.includes(normalized);
}

function selectPickerPath(path: string): void {
  const normalized = normalizeScopePath(path);
  if (!canPickPath(normalized)) return;
  form.value.roots = [...form.value.roots, normalized];
  closeDirectoryPicker();
}

onMounted(() => {
  void reload();
});
</script>

<style scoped>
.settings-content > .settings-table-panel {
  padding: 0;
  overflow: hidden;
}

.settings-table-panel {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.passwords-header-actions {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  flex-wrap: wrap;
}

.passwords-note {
  display: flex;
  align-items: flex-start;
  gap: 0.35rem;
  margin: 0;
  padding: var(--vf-space-3) var(--vf-cell-padding-x);
  border-bottom: 1px solid var(--vf-border-weak);
  background: var(--vf-surface-2);
  color: var(--vf-text-muted);
  font-size: 0.8rem;
  line-height: 1.5;
}

.passwords-note svg {
  flex: 0 0 auto;
  margin-top: 0.12rem;
}

.settings-table-panel > .settings-table-state {
  padding: var(--vf-space-4);
}

.passwords-table-wrap {
  width: 100%;
  overflow-x: auto;
}

.passwords-table {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
  font-size: 0.875rem;
}

.passwords-table th {
  height: var(--vf-table-head-height);
  padding: 0 var(--vf-cell-padding-x);
  border-bottom: 1px solid var(--vf-border-weak);
  background: var(--vf-surface-2);
  color: var(--vf-text-muted);
  font-size: 0.8125rem;
  font-weight: 600;
  text-align: left;
  white-space: nowrap;
  /* 与其它表格一致：垂直居中 + 固定行高，否则 baseline 对齐会让文字贴近下边 */
  vertical-align: middle;
  line-height: 1.35;
}

.passwords-table td {
  height: var(--vf-row-height);
  padding: 0 var(--vf-cell-padding-x);
  border-bottom: 1px solid var(--vf-border-weak);
  vertical-align: middle;
  overflow: hidden;
  text-overflow: ellipsis;
}

.passwords-table th:first-child,
.passwords-table td:first-child {
  padding-left: var(--vf-cell-padding-x);
}

.passwords-table th:last-child,
.passwords-table td:last-child {
  padding-right: var(--vf-cell-padding-x);
}

.passwords-table tbody tr:last-child td {
  border-bottom: none;
}

.passwords-table tbody tr:hover {
  background: var(--vf-row-hover);
}

.passwords-col-name {
  width: 19%;
}

.passwords-col-roots {
  width: 24%;
}

.passwords-col-mode {
  width: 9%;
}

.passwords-col-created {
  width: 15%;
}

.passwords-col-used {
  width: 16%;
}

.passwords-col-actions {
  width: 17%;
  text-align: right;
}

.passwords-name {
  color: var(--vf-text);
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.passwords-muted {
  color: var(--vf-text-muted);
  white-space: nowrap;
}

.passwords-roots {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem;
  min-width: 0;
}

.password-root-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.2rem;
  max-width: 16rem;
  padding: 0.08rem 0.4rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-pill);
  background: var(--vf-surface-2);
  color: var(--vf-text);
  font-size: 0.75rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.password-root-chip.is-editable {
  padding-right: 0.2rem;
}

.password-root-remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.1rem;
  height: 1.1rem;
  padding: 0;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--vf-text-muted);
  cursor: pointer;
}

.password-root-remove:hover,
.password-root-remove:focus-visible {
  background: var(--vf-danger-soft);
  color: var(--vf-danger-text);
}

.password-mode {
  flex: 0 0 auto;
}

.passwords-actions {
  text-align: right;
  white-space: nowrap;
}

.passwords-actions-inner {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 0.15rem;
}

.passwords-action {
  min-height: 1.9rem;
  padding: 0 0.5rem;
  font-size: 0.8rem;
}

.passwords-delete {
  color: var(--vf-danger-text);
}

/* 窄屏：每行一张卡，表格列不横向溢出 */
.passwords-cards {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  padding: 0.75rem;
}

.passwords-card {
  padding: 0.75rem 0.8rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface);
}

.passwords-card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  min-width: 0;
}

.passwords-card-meta {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr));
  gap: 0.55rem 0.9rem;
  margin: 0.6rem 0 0.7rem;
}

.passwords-card-meta > div {
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
  min-width: 0;
}

.passwords-card-meta dt {
  color: var(--vf-text-subtle);
  font-size: 0.7rem;
}

.passwords-card-meta dd {
  margin: 0;
  color: var(--vf-text);
  font-size: 0.8rem;
  word-break: break-word;
}

.passwords-card-roots {
  grid-column: 1 / -1;
}

.passwords-card-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.3rem;
}

.passwords-form {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  min-width: min(460px, 100%);
}

.passwords-label {
  color: var(--vf-text-muted);
  font-size: 0.8rem;
  font-weight: 600;
}

.passwords-roots-editor {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.3rem;
  min-height: 2rem;
  padding: 0.3rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface-2);
}

.passwords-add-root {
  font-size: 0.8rem;
}

.passwords-hint {
  margin: 0;
  color: var(--vf-text-subtle);
  font-size: 0.75rem;
}

.passwords-write-toggle {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  margin-top: 0.35rem;
  color: var(--vf-text);
  font-size: 0.875rem;
}

.passwords-error {
  margin: 0.25rem 0 0;
  color: var(--vf-danger-text);
  font-size: 0.8rem;
}

.directory-picker {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  min-width: min(480px, 100%);
}

.directory-picker-path {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.directory-picker-current {
  padding: 0.15rem 0.4rem;
  border-radius: var(--vf-radius-xs);
  background: var(--vf-surface-2);
  color: var(--vf-text);
  font-size: 0.8rem;
  word-break: break-all;
}

.directory-picker-list {
  max-height: 18rem;
  margin: 0;
  padding: 0;
  overflow-y: auto;
  list-style: none;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
}

.directory-picker-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  padding: 0.25rem 0.5rem;
  border-bottom: 1px solid var(--vf-border-weak);
}

.directory-picker-row:last-child {
  border-bottom: none;
}

.directory-picker-enter {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  flex: 1 1 auto;
  min-width: 0;
  padding: 0.35rem 0.4rem;
  border: none;
  border-radius: var(--vf-radius-sm);
  background: transparent;
  color: var(--vf-text);
  font-size: 0.875rem;
  text-align: left;
  cursor: pointer;
}

.directory-picker-enter:hover,
.directory-picker-enter:focus-visible {
  background: var(--vf-row-hover);
  color: var(--vf-text);
}

.directory-picker-enter span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.directory-picker-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  flex-wrap: wrap;
}
</style>
