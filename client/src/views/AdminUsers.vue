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
            <button class="button is-light" :class="{ 'is-loading': loading }" @click="reload">
              刷新
            </button>
          </div>
        </div>
      </div>

      <div v-if="error" class="notification is-danger is-light">
        {{ error }}
      </div>

      <div class="table-container">
        <table class="table is-fullwidth is-striped">
          <thead>
            <tr>
              <th>用户名</th>
              <th>角色</th>
              <th>状态</th>
              <th>创建时间</th>
              <th style="width: 220px">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="u in users" :key="u.id">
              <td>
                <code>{{ u.username }}</code>
              </td>
              <td>
                <div class="select is-small">
                  <select
                    :value="u.role"
                    :disabled="loading"
                    @change="onChangeRole(u.id, ($event.target as HTMLSelectElement).value)"
                  >
                    <option value="user">user</option>
                    <option value="admin">admin</option>
                  </select>
                </div>
              </td>
              <td>
                <span
                  class="tag"
                  :class="u.disabled ? 'is-warning' : 'is-success'"
                >
                  {{ u.disabled ? '禁用' : '正常' }}
                </span>
              </td>
              <td>
                <span class="is-size-7">{{ u.createdAt }}</span>
              </td>
              <td>
                <div class="buttons are-small">
                  <button
                    class="button is-light"
                    :disabled="loading"
                    @click="toggleDisabled(u)"
                  >
                    {{ u.disabled ? '启用' : '禁用' }}
                  </button>
                </div>
              </td>
            </tr>

            <tr v-if="!loading && users.length === 0">
              <td colspan="5" class="has-text-centered has-text-grey">
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
import { onMounted, ref } from "vue";
import { authService, type AdminUser } from "../services/auth.service";
import { useAppStore } from "../stores/app.store";

const app = useAppStore();

const users = ref<AdminUser[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);

async function reload() {
  loading.value = true;
  error.value = null;
  try {
    const res = await authService.listUsers();
    if (!res.success) {
      error.value = res.error || "加载失败";
      users.value = [];
      return;
    }
    users.value = res.data?.users ?? [];
  } catch (e) {
    error.value = e instanceof Error ? e.message : "加载失败";
    users.value = [];
  } finally {
    loading.value = false;
  }
}

async function onChangeRole(userId: string, nextRole: string) {
  if (nextRole !== "admin" && nextRole !== "user") return;
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

onMounted(() => {
  void reload();
});
</script>
