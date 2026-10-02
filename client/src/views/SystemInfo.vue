<template>
  <main class="vf-page-card system-info-page">
    <header class="system-info-header">
      <div>
        <p class="system-info-eyebrow">管理控制台</p>
        <h1 class="vf-page-title">系统信息</h1>
        <p class="vf-page-subtitle">
          查看服务运行状态、协议配置和所有用户的存储用量
        </p>
      </div>
      <div class="system-info-header-actions">
        <button
          class="vf-ghost-button"
          type="button"
          :disabled="loading"
          @click="load"
        >
          <span :class="{ 'is-spinning': loading }" aria-hidden="true">↻</span>
          <span>刷新</span>
        </button>
        <RouterLink class="vf-ghost-button" to="/">返回文件</RouterLink>
      </div>
    </header>

    <p v-if="error" class="system-info-error" role="alert">{{ error }}</p>

    <SkeletonList v-if="loading && !info" :rows="4" label="加载系统信息" />

    <template v-if="info">
      <section class="system-info-stats" aria-label="系统统计">
        <article class="system-info-stat">
          <span class="system-info-stat-label">用户</span>
          <strong>{{ numberLabel(info.storage.user_count) }}</strong>
          <span class="system-info-stat-detail">个账号</span>
        </article>
        <article class="system-info-stat is-accent">
          <span class="system-info-stat-label">当前存储用量</span>
          <strong>{{ formatSize(info.storage.total_bytes) }}</strong>
          <span class="system-info-stat-detail">
            {{ numberLabel(info.storage.file_count) }} 个文件
          </span>
        </article>
        <article class="system-info-stat">
          <span class="system-info-stat-label">目录</span>
          <strong>{{ numberLabel(info.storage.directory_count) }}</strong>
          <span class="system-info-stat-detail">个目录</span>
        </article>
        <article class="system-info-stat">
          <span class="system-info-stat-label">启用协议</span>
          <strong>{{ enabledProtocolCount }}</strong>
          <span class="system-info-stat-detail">项协议配置已启用</span>
        </article>
      </section>

      <div class="system-info-main-grid">
        <section class="system-info-panel" aria-labelledby="protocols-title">
          <div class="system-info-section-heading">
            <div>
              <h2 id="protocols-title" class="system-info-card-title">
                接入协议
              </h2>
              <p class="system-info-section-description">
                服务启动时读取的协议配置
              </p>
            </div>
            <span class="system-info-section-count">
              {{ enabledProtocolCount }} / {{ info.protocols.length }}
            </span>
          </div>

          <p class="system-info-note">
            状态反映启动配置；监听器是否成功启动，请结合服务日志确认。
          </p>

          <ul class="system-info-protocols">
            <li
              v-for="protocol in info.protocols"
              :key="protocol.id"
              class="system-info-protocol"
              :class="{ 'is-disabled': !protocol.enabled }"
            >
              <div class="system-info-protocol-heading">
                <div class="system-info-protocol-name-wrap">
                  <span
                    class="system-info-protocol-indicator"
                    :class="protocol.enabled ? 'is-enabled' : 'is-disabled'"
                    aria-hidden="true"
                  />
                  <div>
                    <h3>{{ protocolLabel(protocol.id) }}</h3>
                    <p>{{ protocolDescription(protocol.id) }}</p>
                  </div>
                </div>
                <span
                  class="system-info-status"
                  :class="protocol.enabled ? 'is-enabled' : 'is-disabled'"
                >
                  {{ protocol.enabled ? "已启用" : "未启用" }}
                </span>
              </div>

              <dl class="system-info-protocol-details">
                <template v-if="protocol.enabled">
                  <dt>监听地址</dt>
                  <dd class="system-info-mono">{{ protocol.bind }}</dd>
                  <dt>接入方式</dt>
                  <dd>{{ protocolMode(protocol) }}</dd>
                  <template v-if="protocol.passive_ports">
                    <dt>被动端口</dt>
                    <dd class="system-info-mono">
                      {{ protocol.passive_ports }}
                    </dd>
                  </template>
                  <template v-if="protocol.module">
                    <dt>模块</dt>
                    <dd class="system-info-mono">{{ protocol.module }}</dd>
                  </template>
                  <template v-if="protocol.writable !== null">
                    <dt>写入权限</dt>
                    <dd>{{ protocol.writable ? "允许写入" : "只读" }}</dd>
                  </template>
                </template>
                <template v-else>
                  <dt>配置</dt>
                  <dd>已关闭</dd>
                </template>
              </dl>
            </li>
          </ul>
        </section>

        <section
          class="system-info-panel system-info-runtime"
          aria-label="运行信息"
        >
          <div class="system-info-section-heading">
            <div>
              <h2 class="system-info-card-title">运行信息</h2>
              <p class="system-info-section-description">服务实例与构建信息</p>
            </div>
            <span class="system-info-version">{{ info.version }}</span>
          </div>
          <dl class="system-info-runtime-list">
            <div>
              <dt>运行环境</dt>
              <dd>{{ runtimeLabel }}</dd>
            </div>
            <div>
              <dt>启动时间</dt>
              <dd>{{ startedAtLabel }}</dd>
            </div>
            <div>
              <dt>已运行</dt>
              <dd>{{ uptimeLabel }}</dd>
            </div>
          </dl>
        </section>
      </div>

      <section
        class="system-info-panel system-info-storage"
        aria-labelledby="storage-title"
      >
        <div class="system-info-section-heading system-info-storage-heading">
          <div>
            <h2 id="storage-title" class="system-info-card-title">
              存储与用量
            </h2>
            <p class="system-info-section-description">
              按用户汇总所有命名空间当前版本的文件大小（逻辑用量）
            </p>
          </div>
          <label class="system-info-search">
            <span class="sr-only">搜索用户名</span>
            <input
              v-model.trim="searchQuery"
              type="search"
              placeholder="搜索用户"
              aria-label="搜索用户名"
            />
          </label>
        </div>

        <div v-if="filteredUsers.length" class="system-info-table-wrap">
          <table class="system-info-table">
            <thead>
              <tr>
                <th scope="col">用户</th>
                <th scope="col">角色 / 状态</th>
                <th scope="col">文件与目录</th>
                <th scope="col" class="system-info-usage-column">存储用量</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="user in filteredUsers" :key="user.user_id">
                <td>
                  <div class="system-info-user">
                    <span class="system-info-avatar" aria-hidden="true">
                      {{ (user.username || "?").slice(0, 1).toUpperCase() }}
                    </span>
                    <span>{{ user.username }}</span>
                  </div>
                </td>
                <td>
                  <div class="system-info-user-status">
                    <span class="system-info-role">{{
                      roleLabel(user.role)
                    }}</span>
                    <span
                      class="system-info-account-state"
                      :class="user.disabled ? 'is-disabled' : 'is-enabled'"
                    >
                      {{ user.disabled ? "已停用" : "正常" }}
                    </span>
                  </div>
                </td>
                <td>
                  <span class="system-info-count-line">
                    {{ numberLabel(user.file_count) }} 个文件
                  </span>
                  <span class="system-info-muted-line">
                    {{ numberLabel(user.directory_count) }} 个目录
                  </span>
                </td>
                <td class="system-info-usage-cell">
                  <div class="system-info-usage-value">
                    <strong>{{ formatSize(user.total_bytes) }}</strong>
                    <span>{{ usagePercent(user.total_bytes) }}%</span>
                  </div>
                  <div
                    class="system-info-usage-track"
                    role="meter"
                    :aria-label="`${user.username} 的存储用量占比`"
                    aria-valuemin="0"
                    aria-valuemax="100"
                    :aria-valuenow="usagePercent(user.total_bytes)"
                  >
                    <span
                      :style="{ width: `${usagePercent(user.total_bytes)}%` }"
                    />
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <p v-else class="system-info-empty">
          {{ searchQuery ? "没有匹配的用户" : "暂无用户用量" }}
        </p>
      </section>
    </template>
  </main>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { RouterLink } from "vue-router";
import SkeletonList from "../components/common/SkeletonList.vue";
import { authService } from "../services/auth.service";
import type {
  SystemInfoPayload,
  SystemProtocolInfo,
  SystemUserStorageUsage,
} from "../services/auth.service";
import { useAppStore } from "../stores/app.store";
import { formatSize } from "../utils/filePresentation";

const app = useAppStore();
const info = ref<SystemInfoPayload | null>(null);
const loading = ref(false);
const error = ref("");
const searchQuery = ref("");

const runtimeLabel = computed(() =>
  info.value ? `${info.value.os}/${info.value.arch}` : "—",
);
const startedAtLabel = computed(() => {
  const value = info.value?.started_at;
  if (!value) return "—";
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
});
const uptimeLabel = computed(() => {
  const seconds = info.value?.uptime_secs ?? 0;
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  if (days > 0) return `${days} 天 ${hours} 小时`;
  if (hours > 0) return `${hours} 小时 ${minutes} 分`;
  return `${minutes} 分 ${seconds % 60} 秒`;
});
const enabledProtocolCount = computed(
  () =>
    info.value?.protocols.filter((protocol) => protocol.enabled).length ?? 0,
);
const filteredUsers = computed(() => {
  const users = info.value?.storage.users ?? [];
  const query = searchQuery.value.toLocaleLowerCase();
  return query
    ? users.filter((user) => user.username.toLocaleLowerCase().includes(query))
    : users;
});

function numberLabel(value: number) {
  return new Intl.NumberFormat("zh-CN").format(value);
}

function protocolLabel(id: SystemProtocolInfo["id"]) {
  const labels: Record<SystemProtocolInfo["id"], string> = {
    http: "HTTP API",
    webdav: "WebDAV",
    ftps: "FTPS",
    s3: "S3 兼容存储",
    rsync: "rsync",
  };
  return labels[id];
}

function protocolDescription(id: SystemProtocolInfo["id"]) {
  const descriptions: Record<SystemProtocolInfo["id"], string> = {
    http: "网页管理与 REST API",
    webdav: "兼容网盘与文件挂载客户端",
    ftps: "基于 TLS 加密的 FTP 文件导入",
    s3: "S3 兼容对象存储 API",
    rsync: "rsync daemon 文件同步",
  };
  return descriptions[id];
}

function protocolMode(protocol: SystemProtocolInfo) {
  if (protocol.embedded) {
    return protocol.mount_path
      ? `共享 HTTP 端口 · ${protocol.mount_path}`
      : "共享 HTTP 端口";
  }
  return "独立监听端口";
}

function roleLabel(role: SystemUserStorageUsage["role"]) {
  const labels = { admin: "管理员", manager: "管理者", user: "普通用户" };
  return labels[role];
}

function usagePercent(bytes: number) {
  const totalBytes = info.value?.storage.total_bytes ?? 0;
  if (totalBytes <= 0 || bytes <= 0) return 0;
  return Math.round((bytes / totalBytes) * 100);
}

async function load() {
  loading.value = true;
  error.value = "";
  try {
    const response = await authService.systemInfo();
    if (response.success && response.data) {
      info.value = response.data;
    } else {
      error.value = response.error || "加载系统信息失败";
      app.error(error.value);
    }
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : "加载系统信息失败";
    app.error(error.value);
  } finally {
    loading.value = false;
  }
}

onMounted(load);
</script>

<style scoped>
.system-info-page {
  --system-info-accent: var(--vf-accent, #5267df);
  --system-info-good: var(--vf-success, #24875a);
  --system-info-muted: var(--vf-text-muted);
  max-width: 1120px;
  margin: 0 auto;
}

.system-info-header,
.system-info-section-heading {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
}

.system-info-header {
  margin-bottom: 1.4rem;
}

.system-info-eyebrow {
  margin: 0 0 0.35rem;
  color: var(--system-info-accent);
  font-size: 0.72rem;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.system-info-header .vf-page-subtitle {
  margin-top: 0.35rem;
}

.system-info-header-actions {
  display: flex;
  flex-shrink: 0;
  gap: 0.55rem;
}

.is-spinning {
  display: inline-block;
  animation: system-info-spin 1s linear infinite;
  font-size: 1.15rem;
  line-height: 1;
}

@keyframes system-info-spin {
  to {
    transform: rotate(360deg);
  }
}

.system-info-error {
  margin-bottom: 1rem;
  padding: 0.8rem 1rem;
  border: 1px solid var(--vf-danger-border, #e9b8b8);
  border-radius: var(--vf-radius);
  background: var(--vf-danger-soft, #fff5f5);
  color: var(--vf-danger-text, #a83232);
}

.system-info-stats {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 0.8rem;
  margin-bottom: 1rem;
}

.system-info-stat,
.system-info-panel {
  min-width: 0;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius);
  background: var(--vf-surface-raised);
  box-shadow: var(--vf-shadow-card);
}

.system-info-stat {
  display: flex;
  min-height: 112px;
  flex-direction: column;
  align-items: flex-start;
  justify-content: center;
  gap: 0.22rem;
  padding: 1rem 1.1rem;
}

.system-info-stat.is-accent {
  border-color: color-mix(
    in srgb,
    var(--system-info-accent) 34%,
    var(--vf-border-weak)
  );
  background: color-mix(
    in srgb,
    var(--system-info-accent) 5%,
    var(--vf-surface-raised)
  );
}

.system-info-stat-label,
.system-info-stat-detail {
  color: var(--system-info-muted);
  font-size: 0.77rem;
}

.system-info-stat strong {
  color: var(--vf-text-strong);
  font-size: clamp(1.15rem, 2vw, 1.55rem);
  font-variant-numeric: tabular-nums;
  line-height: 1.25;
}

.system-info-main-grid {
  display: grid;
  grid-template-columns: minmax(0, 1.65fr) minmax(250px, 0.85fr);
  align-items: start;
  gap: 1rem;
  margin-bottom: 1rem;
}

.system-info-panel {
  padding: 1.15rem 1.2rem;
}

.system-info-card-title {
  margin: 0;
  color: var(--vf-text-strong);
  font-size: 0.98rem;
  font-weight: 650;
}

.system-info-section-description {
  margin: 0.28rem 0 0;
  color: var(--system-info-muted);
  font-size: 0.78rem;
  line-height: 1.45;
}

.system-info-section-count,
.system-info-version {
  flex-shrink: 0;
  padding: 0.32rem 0.58rem;
  border-radius: 999px;
  background: var(--vf-surface-sunken, var(--vf-surface));
  color: var(--vf-text-muted);
  font-size: 0.72rem;
  font-variant-numeric: tabular-nums;
}

.system-info-note {
  margin: 0.85rem 0 0;
  color: var(--system-info-muted);
  font-size: 0.73rem;
  line-height: 1.5;
}

.system-info-protocols {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0.65rem;
  margin: 0.75rem 0 0;
  padding: 0;
  list-style: none;
}

.system-info-protocol {
  min-width: 0;
  padding: 0.8rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: calc(var(--vf-radius) - 3px);
  background: var(--vf-surface, #fff);
}

.system-info-protocol.is-disabled {
  background: color-mix(
    in srgb,
    var(--vf-surface) 75%,
    var(--vf-surface-sunken, #f3f4f6)
  );
}

.system-info-protocol-heading,
.system-info-protocol-name-wrap {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.55rem;
}

.system-info-protocol-name-wrap {
  justify-content: flex-start;
  min-width: 0;
}

.system-info-protocol-name-wrap h3 {
  margin: 0;
  color: var(--vf-text-strong);
  font-size: 0.84rem;
  font-weight: 650;
}

.system-info-protocol-name-wrap p {
  margin: 0.14rem 0 0;
  color: var(--system-info-muted);
  font-size: 0.69rem;
  line-height: 1.4;
}

.system-info-protocol-indicator {
  width: 0.5rem;
  height: 0.5rem;
  flex: 0 0 0.5rem;
  margin-top: 0.29rem;
  border-radius: 50%;
  background: var(--vf-text-muted);
}

.system-info-protocol-indicator.is-enabled {
  background: var(--system-info-good);
  box-shadow: 0 0 0 3px
    color-mix(in srgb, var(--system-info-good) 13%, transparent);
}

.system-info-status,
.system-info-account-state {
  flex-shrink: 0;
  padding: 0.19rem 0.42rem;
  border-radius: 999px;
  font-size: 0.65rem;
  line-height: 1.4;
}

.system-info-status.is-enabled,
.system-info-account-state.is-enabled {
  background: color-mix(in srgb, var(--system-info-good) 10%, transparent);
  color: var(--system-info-good);
}

.system-info-status.is-disabled,
.system-info-account-state.is-disabled {
  background: var(--vf-surface-sunken, #f1f2f4);
  color: var(--vf-text-muted);
}

.system-info-protocol-details {
  display: grid;
  grid-template-columns: 4.5rem minmax(0, 1fr);
  gap: 0.27rem 0.5rem;
  margin: 0.75rem 0 0;
  font-size: 0.71rem;
  line-height: 1.45;
}

.system-info-protocol-details dt {
  color: var(--system-info-muted);
}

.system-info-protocol-details dd {
  min-width: 0;
  margin: 0;
  color: var(--vf-text);
  overflow-wrap: anywhere;
}

.system-info-mono {
  font-family: var(--vf-font-mono, ui-monospace, monospace);
  font-variant-numeric: tabular-nums;
}

.system-info-runtime-list {
  display: grid;
  gap: 0;
  margin: 1rem 0 0;
}

.system-info-runtime-list div {
  display: grid;
  grid-template-columns: 5rem minmax(0, 1fr);
  gap: 0.7rem;
  padding: 0.76rem 0;
  border-bottom: 1px solid var(--vf-border-weak);
  font-size: 0.78rem;
}

.system-info-runtime-list div:last-child {
  border-bottom: 0;
}

.system-info-runtime-list dt {
  color: var(--system-info-muted);
}

.system-info-runtime-list dd {
  min-width: 0;
  margin: 0;
  color: var(--vf-text);
  font-variant-numeric: tabular-nums;
  overflow-wrap: anywhere;
}

.system-info-storage {
  padding-bottom: 0.6rem;
}

.system-info-storage-heading {
  align-items: center;
  margin-bottom: 0.9rem;
}

.system-info-search input {
  width: min(220px, 42vw);
  height: 2.25rem;
  padding: 0 0.7rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm, 6px);
  background: var(--vf-surface);
  color: var(--vf-text);
  font: inherit;
  font-size: 0.78rem;
}

.system-info-search input:focus-visible {
  outline: 2px solid
    color-mix(in srgb, var(--system-info-accent) 60%, transparent);
  outline-offset: 1px;
}

.system-info-table-wrap {
  overflow-x: auto;
}

.system-info-table {
  width: 100%;
  border-collapse: collapse;
  text-align: left;
  font-size: 0.79rem;
}

.system-info-table th {
  padding: 0.62rem 0.7rem;
  border-bottom: 1px solid var(--vf-border-weak);
  color: var(--system-info-muted);
  font-size: 0.68rem;
  font-weight: 600;
  white-space: nowrap;
}

.system-info-table td {
  padding: 0.72rem 0.7rem;
  border-bottom: 1px solid var(--vf-border-weak);
  color: var(--vf-text);
  vertical-align: middle;
}

.system-info-table tbody tr:last-child td {
  border-bottom: 0;
}

.system-info-user,
.system-info-user-status {
  display: flex;
  align-items: center;
  gap: 0.52rem;
}

.system-info-user {
  color: var(--vf-text-strong);
  font-weight: 580;
}

.system-info-avatar {
  display: inline-grid;
  width: 1.8rem;
  height: 1.8rem;
  flex: 0 0 1.8rem;
  place-items: center;
  border-radius: 50%;
  background: color-mix(
    in srgb,
    var(--system-info-accent) 12%,
    var(--vf-surface)
  );
  color: var(--system-info-accent);
  font-size: 0.72rem;
  font-weight: 700;
}

.system-info-user-status {
  flex-wrap: wrap;
  gap: 0.35rem;
}

.system-info-role {
  color: var(--vf-text);
}

.system-info-count-line,
.system-info-muted-line {
  display: block;
  white-space: nowrap;
}

.system-info-count-line {
  color: var(--vf-text);
}

.system-info-muted-line {
  margin-top: 0.16rem;
  color: var(--system-info-muted);
  font-size: 0.7rem;
}

.system-info-usage-column {
  width: 31%;
}

.system-info-usage-value {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 0.7rem;
  font-variant-numeric: tabular-nums;
}

.system-info-usage-value strong {
  color: var(--vf-text-strong);
  font-size: 0.8rem;
  font-weight: 620;
}

.system-info-usage-value span {
  color: var(--system-info-muted);
  font-size: 0.68rem;
}

.system-info-usage-track {
  height: 0.32rem;
  margin-top: 0.43rem;
  overflow: hidden;
  border-radius: 999px;
  background: var(--vf-surface-sunken, #eff0f3);
}

.system-info-usage-track span {
  display: block;
  height: 100%;
  border-radius: inherit;
  background: var(--system-info-accent);
  transition: width 180ms ease;
}

.system-info-empty {
  margin: 0;
  padding: 1.6rem 1rem;
  color: var(--system-info-muted);
  text-align: center;
  font-size: 0.82rem;
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}

@media (max-width: 800px) {
  .system-info-stats {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .system-info-main-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}

@media (max-width: 560px) {
  .system-info-header {
    align-items: flex-start;
    flex-direction: column;
  }

  .system-info-header-actions {
    width: 100%;
  }

  .system-info-header-actions > * {
    flex: 1;
    justify-content: center;
  }

  .system-info-protocols {
    grid-template-columns: minmax(0, 1fr);
  }

  .system-info-panel {
    padding: 0.95rem;
  }

  .system-info-storage-heading {
    align-items: flex-start;
    flex-direction: column;
  }

  .system-info-search,
  .system-info-search input {
    width: 100%;
  }

  .system-info-table {
    min-width: 580px;
  }

  .system-info-table th,
  .system-info-table td {
    padding-right: 0.5rem;
    padding-left: 0.5rem;
  }
}

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
