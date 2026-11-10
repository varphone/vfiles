<template>
  <AdminLayout
    title="概览"
    description="服务状态、存储用量与最近的账号活动。"
    active="overview"
  >
    <template #actions>
      <button class="vf-ghost-button" type="button" :disabled="loading" @click="reload">
        <IconRefresh :size="15" :class="{ 'is-spinning': loading }" />
        <span>刷新</span>
      </button>
    </template>

    <div v-if="error" class="admin-overview-error" role="alert">
      <IconAlertCircle :size="16" />
      <span>{{ error }}</span>
      <button class="vf-ghost-button" type="button" @click="reload">重试</button>
    </div>

    <SkeletonList
      v-if="loading && !overview && !storage"
      :rows="4"
      label="加载概览"
      class="admin-overview-skeleton"
    />

    <section v-else class="admin-stats" aria-label="关键指标">
      <StatCard
        label="用户"
        :value="overview?.userCount ?? storage?.user_count ?? '—'"
        :hint="overview ? `${overview.activeUserCount} 个启用中` : '已创建的账号'"
        :icon="IconUsers"
      />
      <StatCard
        label="文件"
        :value="overview?.fileCount ?? storage?.file_count ?? '—'"
        hint="全部工作区"
        :icon="IconFile"
      />
      <StatCard
        label="目录"
        :value="overview?.directoryCount ?? storage?.directory_count ?? '—'"
        hint="全部工作区"
        :icon="IconFolder"
      />
      <StatCard
        label="存储用量"
        :value="
          overview
            ? formatSize(overview.totalBytes)
            : storage
              ? formatSize(storage.total_bytes)
              : '—'
        "
        hint="逻辑大小"
        :icon="IconDatabase"
      />
    </section>

    <div class="admin-grid">
      <SectionCard title="服务状态" description="各协议的运行状态与监听地址">
        <template #actions>
          <RouterLink class="vf-ghost-button" to="/admin/system">
            <span>系统详情</span>
            <IconChevronRight :size="15" />
          </RouterLink>
        </template>
        <ul class="admin-protocols">
          <li v-for="protocol in protocols" :key="protocol.id" class="admin-protocol">
            <div class="admin-protocol-main">
              <span class="admin-protocol-name">{{ protocolLabel(protocol.id) }}</span>
              <span class="admin-protocol-bind">{{ protocol.bind }}</span>
            </div>
            <Badge :tone="protocol.enabled ? 'success' : 'muted'" dot>
              {{ protocol.enabled ? "运行中" : "未启用" }}
            </Badge>
          </li>
          <li v-if="!protocols.length" class="admin-muted">暂无协议信息</li>
        </ul>
      </SectionCard>

      <SectionCard title="系统" description="版本与运行时间">
        <dl class="admin-facts">
          <div>
            <dt>版本</dt>
            <dd class="admin-facts-strong">{{ info?.version || "—" }}</dd>
          </div>
          <div>
            <dt>运行时间</dt>
            <dd>{{ info ? formatUptime(info.uptime_secs) : "—" }}</dd>
          </div>
          <div>
            <dt>平台</dt>
            <dd>{{ info ? `${info.os} / ${info.arch}` : "—" }}</dd>
          </div>
          <div>
            <dt>启动时间</dt>
            <dd>{{ info ? formatDate(info.started_at) : "—" }}</dd>
          </div>
        </dl>
      </SectionCard>
    </div>

    <SectionCard title="最近活动" description="最新的账号与文件操作" flush>
      <template #actions>
        <RouterLink class="vf-ghost-button" to="/admin/audit">
          <span>全部日志</span>
          <IconChevronRight :size="15" />
        </RouterLink>
      </template>
      <ul v-if="recent.length" class="admin-activity">
        <li v-for="entry in recent" :key="entry.id" class="admin-activity-item">
          <span
            class="admin-activity-icon"
            :class="entry.result === 'failure' ? 'is-failure' : 'is-success'"
            aria-hidden="true"
          >
            <IconAlertTriangle v-if="entry.result === 'failure'" :size="15" />
            <IconCheck v-else :size="15" />
          </span>
          <div class="admin-activity-main">
            <p class="admin-activity-title">
              <strong>{{ entry.username || "系统" }}</strong>
              <span class="admin-activity-action">{{ actionLabel(entry.action) }}</span>
              <span v-if="entry.target" class="admin-activity-target">{{ entry.target }}</span>
            </p>
            <p class="admin-activity-meta">
              {{ formatDate(entry.created_at) }}
              <template v-if="entry.ip"> · {{ entry.ip }}</template>
            </p>
          </div>
        </li>
      </ul>
      <EmptyState
        v-else
        compact
        :icon="IconUsers"
        title="暂无活动记录"
        hint="登录与文件操作会显示在这里"
      />
    </SectionCard>
  </AdminLayout>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  IconAlertCircle,
  IconAlertTriangle,
  IconCheck,
  IconChevronRight,
  IconDatabase,
  IconFile,
  IconFolder,
  IconRefresh,
  IconUsers,
} from "@tabler/icons-vue";
import EmptyState from "../components/common/EmptyState.vue";
import AdminLayout from "../layouts/AdminLayout.vue";
import SectionCard from "../ui/SectionCard.vue";
import StatCard from "../ui/StatCard.vue";
import Badge from "../ui/Badge.vue";
import { authService, type SystemInfoPayload } from "../services/auth.service";
import { filesService } from "../services/files.service";
import type { AuditLogEntry } from "../types";
import { formatDate, formatSize } from "../utils/filePresentation";

const info = ref<SystemInfoPayload | null>(null);
const recent = ref<AuditLogEntry[]>([]);
const loading = ref(false);
const error = ref("");

const overview = ref<import("../services/auth.service").AdminOverviewPayload | null>(null);
const storage = computed(() => info.value?.storage ?? null);
const protocols = computed(() => info.value?.protocols ?? []);

const PROTOCOL_LABELS: Record<string, string> = {
  http: "HTTP / Web",
  webdav: "WebDAV",
  ftp: "FTP / FTPS",
  s3: "S3",
  rsync: "rsync",
};

function protocolLabel(id: string): string {
  return PROTOCOL_LABELS[id] ?? id.toUpperCase();
}

function actionLabel(action: string): string {
  const labels: Record<string, string> = {
    "login.success": "登录",
    "login.failure": "登录失败",
    "auth.logout": "退出登录",
    "file.upload": "上传",
    "file.download": "下载",
    "file.delete": "删除",
    "file.move": "移动",
    "file.rename": "重命名",
    "file.transfer": "转移",
    "file.restore": "恢复版本",
    "directory.create": "新建目录",
    "share.create": "创建分享",
    "share.disable": "停用分享",
    "share.download": "分享下载",
    "user.create": "创建用户",
    "user.update": "更新用户",
    "user.sessions_revoke": "撤销会话",
    "token.create": "创建令牌",
    "token.revoke": "撤销令牌",
    "credential.create": "创建访问密码",
    "credential.update": "更新访问密码",
    "credential.delete": "删除访问密码",
    "audit.export": "导出审计",
  };
  return labels[action] ?? action;
}

function formatUptime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "—";
  if (seconds < 60) return `${seconds} 秒`;
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return `${days} 天 ${hours} 小时`;
  if (hours > 0) return `${hours} 小时 ${minutes} 分`;
  return `${minutes} 分钟`;
}

async function reload(): Promise<void> {
  loading.value = true;
  error.value = "";
  try {
    const [system, page, summary] = await Promise.all([
      authService.systemInfo(),
      filesService.listAuditLogs({ limit: 6 }),
      authService.adminOverview().catch(() => null),
    ]);
    overview.value = summary;
    if (!system.success) {
      error.value = system.error || "加载系统信息失败";
    } else {
      info.value = system.data ?? null;
    }
    recent.value = page.items ?? [];
  } catch (err) {
    error.value = err instanceof Error ? err.message : "加载概览失败";
  } finally {
    loading.value = false;
  }
}

onMounted(reload);
</script>

<style scoped>
.admin-stats {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 1rem;
}

.admin-grid {
  display: grid;
  grid-template-columns: minmax(0, 1.6fr) minmax(0, 1fr);
  gap: 1rem;
}

.admin-protocols {
  display: flex;
  flex-direction: column;
}

.admin-protocol {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--vf-space-3);
  min-height: var(--vf-row-height);
  padding: 0;
  border-bottom: 1px solid var(--vf-border-weak);
}

.admin-protocol:last-child {
  border-bottom: 0;
}

.admin-protocol-main {
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
  min-width: 0;
}

.admin-protocol-name {
  font-size: var(--vf-text-sm);
  font-weight: 600;
  color: var(--vf-text);
}

.admin-protocol-bind {
  font-size: var(--vf-text-xs);
  color: var(--vf-text-subtle);
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  word-break: break-all;
}

.admin-facts {
  display: flex;
  flex-direction: column;
  gap: var(--vf-space-2);
}

.admin-facts > div {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--vf-space-4);
  padding-bottom: var(--vf-space-2);
  border-bottom: 1px solid var(--vf-border-weak);
}

.admin-facts > div:last-child {
  border-bottom: 0;
  padding-bottom: 0;
}

.admin-facts dt {
  font-size: var(--vf-text-xs);
  color: var(--vf-text-muted);
}

.admin-facts dd {
  margin: 0;
  font-size: var(--vf-text-sm);
  color: var(--vf-text);
  text-align: right;
}

.admin-facts-strong {
  font-weight: 650;
}

.admin-activity {
  display: flex;
  flex-direction: column;
}

.admin-activity-item {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  min-height: var(--vf-row-height);
  padding: var(--vf-space-1) 0;
  border-bottom: 1px solid var(--vf-border-weak);
}

.admin-activity-item:last-child {
  border-bottom: 0;
}

.admin-activity-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  flex: none;
  border-radius: 50%;
}

.admin-activity-icon.is-success {
  background: var(--vf-success-soft);
  color: var(--vf-success-text);
}

.admin-activity-icon.is-failure {
  background: var(--vf-warning-soft);
  color: var(--vf-warning-text);
}

.admin-activity-main {
  min-width: 0;
}

.admin-activity-title {
  margin: 0;
  font-size: var(--vf-text-sm);
  color: var(--vf-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.admin-activity-action {
  margin-left: 0.35rem;
  color: var(--vf-text-muted);
}

.admin-activity-target {
  margin-left: 0.35rem;
  color: var(--vf-text-subtle);
}

.admin-activity-meta {
  margin: 0.15rem 0 0;
  font-size: var(--vf-text-xs);
  color: var(--vf-text-subtle);
}

.admin-muted {
  font-size: var(--vf-text-xs);
  color: var(--vf-text-subtle);
}

.admin-overview-error {
  display: flex;
  align-items: center;
  gap: var(--vf-space-2);
  padding: var(--vf-space-2) var(--vf-space-3);
  border: 1px solid var(--vf-danger-soft);
  border-radius: var(--vf-radius-control);
  background: var(--vf-danger-soft);
  color: var(--vf-danger-text);
  font-size: var(--vf-text-sm);
}

@media screen and (max-width: 1024px) {
  .admin-stats {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .admin-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}

@media screen and (max-width: 600px) {
  .admin-stats {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
