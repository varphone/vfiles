<template>
  <section class="section">
    <div class="container">
      <div class="level">
        <div class="level-left">
          <div class="level-item">
            <h1 class="title is-4 mb-0">用户管理</h1>
          </div>
        </div>
        <div class="level-right">
          <div class="level-item">
            <button
              class="button is-light"
              :class="{ 'is-loading': loading }"
              @click="reload"
            >
              刷新
            </button>
          </div>
        </div>
      </div>

      <div v-if="error" class="notification is-danger is-light">
        {{ error }}
      </div>

      <div
        v-if="unsupportedActionsMessage"
        class="notification is-warning is-light"
      >
        {{ unsupportedActionsMessage }}
      </div>

      <div class="table-container">
        <table class="table is-fullwidth is-striped">
          <thead>
            <tr>
              <th>用户名</th>
              <th>邮箱</th>
              <th>角色</th>
              <th>状态</th>
              <th>创建时间</th>
              <th style="width: 320px">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="u in users" :key="u.id">
              <td>
                <code>{{ u.username }}</code>
              </td>
              <td>
                <div class="field has-addons">
                  <div class="control is-expanded">
                    <input
                      class="input is-small"
                      type="email"
                      placeholder="user@example.com"
                      :disabled="loading || !capabilities.canUpdateEmail || !canManageUser(u)"
                      v-model.trim="emailDraft[u.id]"
                    />
                  </div>
                  <div class="control">
                    <button
                      class="button is-small is-light"
                      :disabled="loading || !capabilities.canUpdateEmail || !canManageUser(u)"
                      @click="saveEmail(u)"
                    >
                      保存
                    </button>
                  </div>
                </div>
              </td>
              <td>
                <div class="select is-small">
                  <select
                    :value="u.role"
                    :disabled="loading || !canManageRole(u)"
                    @change="
                      onChangeRole(
                        u.id,
                        ($event.target as HTMLSelectElement).value,
                      )
                    "
                  >
                    <option value="user">user</option>
                    <option value="manager">manager</option>
                    <option value="admin">admin</option>
                  </select>
                </div>
              </td>
              <td>
                <span
                  class="tag"
                  :class="u.disabled ? 'is-warning' : 'is-success'"
                >
                  {{ u.disabled ? "禁用" : "正常" }}
                </span>
              </td>
              <td>
                <span class="is-size-7">{{ u.createdAt }}</span>
              </td>
              <td>
                <div class="buttons are-small">
                  <button
                    class="button is-light"
                    :disabled="loading || !canManageUser(u)"
                    @click="toggleDisabled(u)"
                  >
                    {{ u.disabled ? "启用" : "禁用" }}
                  </button>

                  <button
                    class="button is-warning is-light"
                    :disabled="loading || auth.user?.id === u.id || !capabilities.canRevokeSessions || !canManageUser(u)"
                    @click="revokeSessions(u)"
                  >
                    强制下线
                  </button>
                </div>
              </td>
            </tr>

            <tr v-if="!loading && users.length === 0">
              <td colspan="6" class="has-text-centered has-text-grey">
                暂无用户
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  authService,
  type AdminUser,
  type AdminUserCapabilities,
} from "../services/auth.service";
import { useAppStore } from "../stores/app.store";
import { useAuthStore } from "../stores/auth.store";

const app = useAppStore();
const auth = useAuthStore();

const users = ref<AdminUser[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const emailDraft = ref<Record<string, string>>({});
const defaultCapabilities: AdminUserCapabilities = {
  canUpdateEmail: true,
  canRevokeSessions: true,
};
const capabilities = ref<AdminUserCapabilities>({ ...defaultCapabilities });

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

async function revokeSessions(u: AdminUser) {
  const ok = window.confirm(
    `确定要强制下线用户 ${u.username} 吗？\n（将使其现有登录立即失效）`,
  );
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
