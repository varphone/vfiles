<template>
  <div class="admin-page">
    <div class="admin-card vf-page-card">
      <!-- 页头：标题 + 概览 + 搜索/刷新（与文件浏览器的卡片语言一致） -->
      <header class="admin-header">
        <div class="admin-header-titles">
          <h1 class="admin-title vf-page-title">用户管理</h1>
          <p class="admin-subtitle vf-page-subtitle">
            共 {{ users.length }} 位用户
            <template v-if="filteredUsers.length !== users.length">
              · 当前显示 {{ filteredUsers.length }} 位
            </template>
          </p>
        </div>

        <div class="admin-header-actions">
          <div class="admin-search">
            <IconSearch :size="16" class="admin-search-icon" />
            <input
              v-model.trim="searchQuery"
              class="input is-small admin-search-input"
              type="search"
              placeholder="搜索用户名或邮箱"
              aria-label="搜索用户名或邮箱"
            />
            <button
              v-if="searchQuery"
              class="vf-icon-button admin-search-clear"
              type="button"
              title="清空搜索"
              aria-label="清空搜索"
              @click="searchQuery = ''"
            >
              <IconX :size="14" />
            </button>
          </div>

          <button
            class="vf-ghost-button admin-refresh"
            type="button"
            :disabled="loading"
            @click="reload"
          >
            <IconRefresh :size="16" :class="{ 'is-spinning': loading }" />
            <span>刷新</span>
          </button>
          <button class="vf-ghost-button" type="button" @click="goFiles">
            <IconFolderOpen :size="16" />
            <span>返回文件</span>
          </button>
        </div>
      </header>

      <div v-if="roleBuckets.length > 1" class="admin-filters">
        <button
          class="vf-ghost-button admin-filter"
          :class="{ 'is-active': roleFilter === 'all' }"
          type="button"
          :aria-pressed="roleFilter === 'all' ? 'true' : 'false'"
          @click="roleFilter = 'all'"
        >
          <span>全部</span>
          <span class="admin-filter-count">{{ users.length }}</span>
        </button>
        <button
          v-for="bucket in roleBuckets"
          :key="bucket.role"
          class="vf-ghost-button admin-filter"
          :class="{ 'is-active': roleFilter === bucket.role }"
          type="button"
          :aria-pressed="roleFilter === bucket.role ? 'true' : 'false'"
          @click="roleFilter = bucket.role"
        >
          <span>{{ bucket.label }}</span>
          <span class="admin-filter-count">{{ bucket.count }}</span>
        </button>
      </div>

      <p v-if="unsupportedActionsMessage" class="admin-note">
        <IconInfoCircle :size="14" />
        <span>{{ unsupportedActionsMessage }}</span>
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
        v-else-if="loading && users.length === 0"
        :rows="4"
        label="加载用户列表"
      />

      <EmptyState
        v-else-if="users.length === 0"
        :icon="IconUsers"
        title="暂无用户"
        hint="创建用户后会在这里显示"
      />

      <EmptyState
        v-else-if="filteredUsers.length === 0"
        :icon="IconSearch"
        title="没有找到匹配的用户"
        hint="换个关键字，或清除角色筛选"
      >
        <template #actions>
          <button class="vf-ghost-button" @click="clearFilters">
            <span>清除筛选</span>
          </button>
        </template>
      </EmptyState>

      <div v-else class="admin-table-wrap">
        <table class="table is-fullwidth is-hoverable admin-table">
          <thead>
            <tr>
              <th :aria-sort="ariaSortFor('username')">
                <button class="vf-th-sort" type="button" title="按用户名排序" @click="toggleSort('username')">
                  <span>用户名</span>
                  <IconChevronUp v-if="sortField === 'username' && sortDirection === 'asc'" :size="14" class="vf-th-sort-icon" aria-hidden="true" />
                  <IconChevronDown v-else-if="sortField === 'username'" :size="14" class="vf-th-sort-icon" aria-hidden="true" />
                </button>
              </th>
              <th>邮箱</th>
              <th class="is-narrow" :aria-sort="ariaSortFor('role')">
                <button class="vf-th-sort" type="button" title="按角色排序" @click="toggleSort('role')">
                  <span>角色</span>
                  <IconChevronUp v-if="sortField === 'role' && sortDirection === 'asc'" :size="14" class="vf-th-sort-icon" aria-hidden="true" />
                  <IconChevronDown v-else-if="sortField === 'role'" :size="14" class="vf-th-sort-icon" aria-hidden="true" />
                </button>
              </th>
              <th class="is-narrow">状态</th>
              <th class="is-narrow" :aria-sort="ariaSortFor('createdAt')">
                <button class="vf-th-sort" type="button" title="按创建时间排序" @click="toggleSort('createdAt')">
                  <span>创建时间</span>
                  <IconChevronUp v-if="sortField === 'createdAt' && sortDirection === 'asc'" :size="14" class="vf-th-sort-icon" aria-hidden="true" />
                  <IconChevronDown v-else-if="sortField === 'createdAt'" :size="14" class="vf-th-sort-icon" aria-hidden="true" />
                </button>
              </th>
              <th class="admin-actions-header">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="u in sortedUsers" :key="u.id">
              <td>
                <div class="admin-user">
                  <span class="admin-avatar" aria-hidden="true">
                    {{ (u.username || "?").slice(0, 1).toUpperCase() }}
                  </span>
                  <div class="admin-user-titles">
                    <span class="admin-user-name">{{ u.username }}</span>
                    <span v-if="u.id === auth.user?.id" class="admin-user-self">
                      当前账号
                    </span>
                  </div>
                </div>
              </td>

              <td>
                <!-- 邮箱：默认只读文本，点铅笔进入行内编辑（避免整列都是输入框） -->
                <div v-if="editingId === u.id" class="admin-email-edit">
                  <input
                    :ref="registerEmailInput"
                    v-model.trim="emailDraft[u.id]"
                    class="input is-small"
                    type="email"
                    placeholder="user@example.com"
                    :aria-label="`${u.username} 的邮箱`"
                    @keydown.enter.prevent="saveEmail(u)"
                    @keydown.esc.prevent="cancelEdit"
                  />
                  <button
                    class="vf-ghost-button is-primary admin-edit-button"
                    type="button"
                    :disabled="loading"
                    @click="saveEmail(u)"
                  >
                    保存
                  </button>
                  <button
                    class="vf-ghost-button admin-edit-button"
                    type="button"
                    @click="cancelEdit"
                  >
                    取消
                  </button>
                </div>
                <div v-else class="admin-email">
                  <span class="admin-email-text" :title="u.email || ''">
                    {{ u.email || "未设置" }}
                  </span>
                  <button
                    v-if="canEditEmail(u)"
                    class="vf-icon-button admin-email-edit-button"
                    type="button"
                    :title="`修改 ${u.username} 的邮箱`"
                    :aria-label="`修改 ${u.username} 的邮箱`"
                    @click="startEdit(u)"
                  >
                    <IconPencil :size="14" />
                  </button>
                </div>
              </td>

              <td class="is-narrow">
                <div class="select is-small">
                  <select
                    :value="u.role"
                    :disabled="loading || !canManageRole(u)"
                    :aria-label="`${u.username} 的角色`"
                    @change="
                      onChangeRole(
                        u.id,
                        ($event.target as HTMLSelectElement).value,
                      )
                    "
                  >
                    <option value="user">普通用户</option>
                    <option value="manager">管理者</option>
                    <option value="admin">管理员</option>
                  </select>
                </div>
              </td>

              <td class="is-narrow">
                <span
                  class="admin-status"
                  :class="u.disabled ? 'is-disabled' : 'is-active'"
                >
                  <span class="admin-status-dot" aria-hidden="true"></span>
                  <span>{{ u.disabled ? "已禁用" : "正常" }}</span>
                </span>
              </td>

              <td class="is-narrow">
                <span class="admin-date" :title="u.createdAt">
                  {{ formatRelativeDate(u.createdAt) }}
                </span>
              </td>

              <td class="admin-actions">
                <div class="admin-actions-group">
                  <button
                    class="vf-ghost-button admin-action"
                    type="button"
                    :disabled="loading || !canManageUser(u)"
                    @click="toggleDisabled(u)"
                  >
                    {{ u.disabled ? "启用" : "禁用" }}
                  </button>
                  <button
                    class="vf-ghost-button is-danger admin-action"
                    type="button"
                    :disabled="
                      loading ||
                      auth.user?.id === u.id ||
                      !capabilities.canRevokeSessions ||
                      !canManageUser(u)
                    "
                    @click="revokeSessions(u)"
                  >
                    强制下线
                  </button>
                  <!-- r107' 用户管理完善：重置密码 + 删除（后端早备 ✓ 前端补动作） -->
                  <button
                    class="vf-ghost-button admin-action"
                    type="button"
                    :disabled="loading || !canManageUser(u)"
                    @click="openResetPassword(u)"
                  >
                    重置密码
                  </button>
                  <button
                    class="vf-ghost-button is-danger admin-action"
                    type="button"
                    :disabled="
                      loading || auth.user?.id === u.id || !canManageUser(u)
                    "
                    @click="removeUser(u)"
                  >
                    删除
                  </button>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
  <!-- r107'：重置密码 Modal（一次输入 ✓ 语义：新密码即时生效） -->
  <Modal :show="!!resetTarget" title="重置密码" @close="resetTarget = null">
    <p class="admin-reset-hint">
      为「{{ resetTarget?.username }}」设置新密码（至少 6 位）：
    </p>
    <input
      v-model="resetInput"
      class="input"
      type="text"
      placeholder="输入新密码（可见，便于转告）"
      aria-label="新密码"
      @keyup.enter="submitResetPassword"
    />
    <template #footer>
      <button class="vf-ghost-button" @click="resetTarget = null">取消</button>
      <button
        class="vf-ghost-button is-primary"
        :disabled="resetBusy || resetInput.length < 6"
        @click="submitResetPassword"
      >
        重置密码
      </button>
    </template>
  </Modal>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import {
  IconAlertCircle,
  IconFolderOpen,
  IconInfoCircle,
  IconPencil,
  IconRefresh,
  IconSearch,
  IconUsers,
  IconX,
  IconChevronUp,
  IconChevronDown,
} from "@tabler/icons-vue";
import {
  authService,
  type AdminUser,
  type AdminUserCapabilities,
} from "../services/auth.service";
import { useAppStore } from "../stores/app.store";
import { confirmDialog } from "../composables/dialog";
import { useAuthStore } from "../stores/auth.store";
import Modal from "../components/common/Modal.vue";
import EmptyState from "../components/common/EmptyState.vue";
import SkeletonList from "../components/common/SkeletonList.vue";
import { formatRelativeDate } from "../utils/filePresentation";

const app = useAppStore();
const auth = useAuthStore();

// 表头排序（r118 ✓ 主流管理表标配 ✓ FileList 同式）
type AdminSortField = "username" | "role" | "createdAt";
const sortField = ref<AdminSortField>("createdAt");
const sortDirection = ref<"asc" | "desc">("desc");

function toggleSort(field: AdminSortField) {
  if (sortField.value === field) {
    sortDirection.value = sortDirection.value === "asc" ? "desc" : "asc";
  } else {
    sortField.value = field;
    sortDirection.value = "asc";
  }
}

function ariaSortFor(field: AdminSortField) {
  if (sortField.value !== field) return "none";
  return sortDirection.value === "asc" ? "ascending" : "descending";
}

const sortedUsers = computed(() => {
  const list = [...filteredUsers.value];
  const dir = sortDirection.value === "asc" ? 1 : -1;
  return list.sort((a, b) => {
    const va = String(a[sortField.value] ?? "");
    const vb = String(b[sortField.value] ?? "");
    return va.localeCompare(vb, "zh-Hans-CN") * dir;
  });
});
const router = useRouter();

/** 与「我的分享」「审计日志」一致：返回文件浏览器。 */
function goFiles() {
  router.push({ path: "/" });
}

const users = ref<AdminUser[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const emailDraft = ref<Record<string, string>>({});
const searchQuery = ref("");
const roleFilter = ref<"all" | AdminUser["role"]>("all");
const editingId = ref<string | null>(null);
const emailInput = ref<HTMLInputElement | null>(null);
const defaultCapabilities: AdminUserCapabilities = {
  canUpdateEmail: true,
  canRevokeSessions: true,
};
const capabilities = ref<AdminUserCapabilities>({ ...defaultCapabilities });

const ROLE_LABELS: Record<AdminUser["role"], string> = {
  admin: "管理员",
  manager: "管理者",
  user: "普通用户",
};

const unsupportedActionsMessage = computed(() => {
  const unsupported: string[] = [];
  if (!capabilities.value.canUpdateEmail) {
    unsupported.push("修改邮箱");
  }
  if (!capabilities.value.canRevokeSessions) {
    unsupported.push("强制下线");
  }

  return unsupported.length > 0
    ? `当前后端暂不支持${unsupported.join("、")}`
    : null;
});

/** 关键字匹配用户名/邮箱/角色中文名，便于快速定位。 */
const filteredUsers = computed(() => {
  const query = searchQuery.value.toLowerCase();
  return users.value.filter((user) => {
    if (roleFilter.value !== "all" && user.role !== roleFilter.value) {
      return false;
    }
    if (!query) return true;
    return (
      user.username.toLowerCase().includes(query) ||
      (user.email ?? "").toLowerCase().includes(query) ||
      (ROLE_LABELS[user.role] ?? "").includes(searchQuery.value)
    );
  });
});

/** 角色筛选 chips：只显示实际存在的角色。 */
const roleBuckets = computed(() => {
  const order: AdminUser["role"][] = ["admin", "manager", "user"];
  return order
    .map((role) => ({
      role,
      label: ROLE_LABELS[role] ?? role,
      count: users.value.filter((user) => user.role === role).length,
    }))
    .filter((bucket) => bucket.count > 0);
});

function clearFilters() {
  searchQuery.value = "";
  roleFilter.value = "all";
}

function canManageUser(user: AdminUser): boolean {
  if (auth.user?.role === "admin") {
    return true;
  }

  if (auth.user?.role === "manager") {
    return user.role === "user";
  }

  return false;
}

function canManageRole(user: AdminUser): boolean {
  return auth.user?.role === "admin" && canManageUser(user);
}

function canEditEmail(user: AdminUser): boolean {
  return capabilities.value.canUpdateEmail && canManageUser(user);
}

function registerEmailInput(element: unknown) {
  if (element instanceof HTMLInputElement) {
    emailInput.value = element;
  }
}

async function startEdit(user: AdminUser) {
  emailDraft.value = { ...emailDraft.value, [user.id]: user.email || "" };
  editingId.value = user.id;
  await nextTick();
  emailInput.value?.focus();
}

function cancelEdit() {
  editingId.value = null;
}

async function reload() {
  loading.value = true;
  error.value = null;
  try {
    const res = await authService.listUsers();
    if (!res.success) {
      error.value = res.error || "加载失败";
      users.value = [];
      capabilities.value = { ...defaultCapabilities };
      return;
    }
    users.value = res.data?.users ?? [];
    capabilities.value = res.data?.capabilities ?? { ...defaultCapabilities };
    emailDraft.value = Object.fromEntries(
      users.value.map((u) => [u.id, u.email || ""]),
    );
  } catch (e) {
    error.value = e instanceof Error ? e.message : "加载失败";
    users.value = [];
    emailDraft.value = {};
    capabilities.value = { ...defaultCapabilities };
  } finally {
    loading.value = false;
  }
}

async function saveEmail(u: AdminUser) {
  const nextEmail = (emailDraft.value[u.id] || "").trim();
  if (!nextEmail) {
    app.error("请输入邮箱");
    return;
  }

  loading.value = true;
  try {
    const res = await authService.setUserEmail(u.id, nextEmail);
    if (!res.success) throw new Error(res.error || "保存邮箱失败");
    editingId.value = null;
    app.success("已保存邮箱");
    await reload();
  } catch (e) {
    app.error(e instanceof Error ? e.message : "保存邮箱失败");
  } finally {
    loading.value = false;
  }
}

async function onChangeRole(userId: string, nextRole: string) {
  if (nextRole !== "admin" && nextRole !== "manager" && nextRole !== "user") {
    return;
  }
  loading.value = true;
  try {
    const res = await authService.setUserRole(userId, nextRole);
    if (!res.success) throw new Error(res.error || "更新角色失败");
    app.success("已更新角色");
    await reload();
  } catch (e) {
    app.error(e instanceof Error ? e.message : "更新角色失败");
  } finally {
    loading.value = false;
  }
}

async function toggleDisabled(u: AdminUser) {
  loading.value = true;
  try {
    const res = await authService.setUserDisabled(u.id, !u.disabled);
    if (!res.success) throw new Error(res.error || "更新状态失败");
    app.success("已更新状态");
    await reload();
  } catch (e) {
    app.error(e instanceof Error ? e.message : "更新状态失败");
  } finally {
    loading.value = false;
  }
}

/** r107'：重置密码（Modal 一次输入 ✓ 新密码直接下发后端）。 */
const resetTarget = ref<AdminUser | null>(null);
const resetInput = ref("");
const resetBusy = ref(false);

function openResetPassword(u: AdminUser) {
  resetTarget.value = u;
  resetInput.value = "";
}

async function submitResetPassword() {
  if (!resetTarget.value || resetInput.value.length < 6) return;
  resetBusy.value = true;
  try {
    const res = await authService.resetUserPassword(
      resetTarget.value.id,
      resetInput.value,
    );
    if (!res.success) throw new Error(res.error || "重置密码失败");
    app.success(`已重置「${resetTarget.value.username}」的密码`);
    resetTarget.value = null;
  } catch (e) {
    app.error(e instanceof Error ? e.message : "重置密码失败");
  } finally {
    resetBusy.value = false;
  }
}

/** r107'：删除用户（确认流 ✓ 不能删己 ✓）。 */
async function removeUser(u: AdminUser) {
  const ok = await confirmDialog({
    title: "删除用户",
    message: `确定删除「${u.username}」？该操作不可撤销。`,
    confirmText: "删除",
  });
  if (!ok) return;
  try {
    const res = await authService.deleteUser(u.id);
    if (!res.success) throw new Error(res.error || "删除用户失败");
    app.success(`已删除「${u.username}」`);
    await reload();
  } catch (e) {
    app.error(e instanceof Error ? e.message : "删除用户失败");
  }
}

async function revokeSessions(u: AdminUser) {
  const ok = await confirmDialog({
    title: "强制下线",
    message: `确定要强制下线用户 ${u.username} 吗？\n（将使其现有登录立即失效）`,
    confirmText: "强制下线",
    danger: true,
  });
  if (!ok) return;

  loading.value = true;
  try {
    const res = await authService.revokeUserSessions(u.id);
    if (!res.success) throw new Error(res.error || "强制下线失败");
    app.success("已强制下线");
    await reload();
  } catch (e) {
    app.error(e instanceof Error ? e.message : "强制下线失败");
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  void reload();
});
</script>

<style scoped>
.admin-page {
  padding: 1.25rem 1rem 3rem;
}

.admin-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.8rem;
  flex-wrap: wrap;
  padding-bottom: 0.8rem;
  border-bottom: 1px solid var(--vf-border-weak);
}

.admin-header-actions {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  flex-wrap: wrap;
}

.admin-search {
  position: relative;
  display: flex;
  align-items: center;
}

.admin-search-icon {
  position: absolute;
  /* Bulma 的 .input 也是定位元素，不抬高层级时图标会被输入框底色盖住 */
  z-index: 1;
  left: 0.5rem;
  color: var(--vf-text-subtle);
  pointer-events: none;
}

.admin-search-input {
  width: min(16rem, 60vw);
  padding-left: 1.75rem;
  padding-right: 1.6rem;
  font-size: 0.8rem;
}

.admin-search-clear {
  position: absolute;
  right: 0.2rem;
}

.admin-refresh {
  min-height: 1.9rem;
  padding: 0 0.6rem;
  font-size: 0.8rem;
}

/* 角色筛选 chips */
.admin-filters {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  flex-wrap: wrap;
  padding: 0.7rem 0 0;
}

.admin-filter {
  gap: 0.3rem;
  /* M3 filter chip 高度 32px（原 28px） */
  min-height: 2rem;
  padding: 0 0.6rem;
  font-size: 0.78rem;
}

.admin-filter-count {
  color: var(--vf-text-subtle);
  font-size: 0.72rem;
}

.admin-filter.is-active .admin-filter-count {
  color: currentColor;
  opacity: 0.75;
}

.admin-note {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  margin: 0.75rem 0 0;
  padding: 0.45rem 0.55rem;
  border: 1px solid var(--vf-warning-line);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-warning-soft);
  color: var(--vf-text);
  font-size: 0.78rem;
}

.admin-table-wrap {
  margin-top: 0.6rem;
  overflow-x: auto;
}

.admin-table {
  font-size: 0.84rem;
}

.admin-table thead th {
  border-bottom: 1px solid var(--vf-border-weak);
  color: var(--vf-text-muted);
  font-size: 0.76rem;
  font-weight: 600;
  white-space: nowrap;
}

.admin-table tbody tr:hover {
  background: var(--vf-surface-hover);
}

.admin-user {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.admin-avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: 0 0 auto;
  width: 1.7rem;
  height: 1.7rem;
  border-radius: 50%;
  background: var(--vf-accent-soft);
  color: var(--vf-accent-text);
  font-size: 0.78rem;
  font-weight: 600;
}

.admin-user-titles {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.admin-user-name {
  color: var(--vf-text-strong);
  font-weight: 600;
}

.admin-user-self {
  color: var(--vf-text-subtle);
  font-size: 0.72rem;
}

.admin-email {
  display: flex;
  align-items: center;
  gap: 0.3rem;
}

.admin-email-text {
  color: var(--vf-text);
}

.admin-email-edit-button {
  opacity: 0;
}

.admin-table tbody tr:hover .admin-email-edit-button,
.admin-email-edit-button:focus-visible {
  opacity: 1;
}

.admin-email-edit {
  display: flex;
  align-items: center;
  gap: 0.3rem;
}

.admin-edit-button {
  min-height: 1.75rem;
  padding: 0 0.5rem;
  font-size: 0.76rem;
}

.admin-status {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  font-size: 0.8rem;
  white-space: nowrap;
}

.admin-status-dot {
  width: 0.45rem;
  height: 0.45rem;
  border-radius: 50%;
}

.admin-status.is-active {
  color: var(--vf-success-text);
}

.admin-status.is-active .admin-status-dot {
  background: var(--vf-success-text);
}

.admin-status.is-disabled {
  color: var(--vf-warning-text);
}

.admin-status.is-disabled .admin-status-dot {
  background: var(--vf-warning-text);
}

.admin-date {
  color: var(--vf-text-subtle);
  font-size: 0.76rem;
  white-space: nowrap;
}

.admin-actions-header {
  text-align: right;
}

.admin-actions {
  text-align: right;
}

.admin-actions-group {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
}

.admin-action {
  min-height: 1.8rem;
  padding: 0 0.55rem;
  font-size: 0.78rem;
  white-space: nowrap;
}

.is-spinning {
  animation: admin-spin 0.9s linear infinite;
}

@keyframes admin-spin {
  to {
    transform: rotate(360deg);
  }
}

@media screen and (max-width: 700px) {
  .admin-page {
    padding: 0.75rem 0.5rem 2rem;
  }

  .admin-card {
    padding: 0.9rem 0.85rem 1.1rem;
  }

  .admin-search-input {
    width: 100%;
  }
}

/* reduced-motion：循环动画降级为静态指示（M3: static loading indicators），
   本文件动画族 admin-spin 0.9s */
@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}


</style>