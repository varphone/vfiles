<template>
  <main class="vf-page-card">
    <!-- 标准工具条（用户令 ✓ admin-header 同式 = 标题左 + **刷新/返回文件右上角** ✓） -->
    <header class="system-info-header">
      <div class="system-info-header-titles">
        <h1 class="vf-page-title">系统信息</h1>
        <p class="vf-page-subtitle">运行时与统计概览（仅管理员可见）</p>
      </div>
      <div class="system-info-header-actions">
        <button
          class="vf-ghost-button system-info-refresh"
          type="button"
          :disabled="loading"
          @click="load"
        >
          刷新
        </button>
        <RouterLink class="vf-ghost-button" to="/">返回文件</RouterLink>
      </div>
    </header>

    <!-- 首载骨架（r117 ✓ 与管理视图体系一致（SkeletonList 首载）） -->
    <SkeletonList
      v-if="loading && !info"
      :rows="4"
      label="加载系统信息"
    />

    <!-- 系统卡（新端点 ✓ 零依赖段） -->
    <section class="system-info-card" aria-label="系统信息">
      <h2 class="system-info-card-title">系统</h2>
      <dl class="system-info-list">
        <dt>版本</dt>
        <dd>{{ info?.version ?? "—" }}</dd>
        <dt>运行时</dt>
        <dd>{{ runtimeLabel }}</dd>
        <dt>启动时间</dt>
        <dd>{{ info?.started_at ?? "—" }}</dd>
        <dt>已运行</dt>
        <dd>{{ uptimeLabel }}</dd>
      </dl>
    </section>

    <template v-if="!loading || info">
    <!-- WebDAV 接入卡（r115 ✓ 通用文件管理生态接入指引 = 商业产品同款（坚果云/Box 式）） -->
    <section class="system-info-card" aria-label="WebDAV 接入">
      <h2 class="system-info-card-title">WebDAV 接入</h2>
      <template v-if="info?.webdav_enabled">
        <dl class="system-info-list">
          <dt>端点地址</dt>
          <dd class="system-info-mono">{{ webdavEndpoint }}</dd>
          <dt>协议方法</dt>
          <dd>PROPFIND / GET / HEAD / PUT / MKCOL / DELETE / MOVE / LOCK / UNLOCK</dd>
        </dl>
        <p class="system-info-hint">
          在 Finder「连接服务器」、rclone、Cyberduck 或 Windows 映射网络驱动器中输入端点地址，
          使用本站账号登录即可挂载读写。用户名密码即本站凭据。
        </p>
        <details class="system-info-details">
          <summary>客户端挂载示例</summary>
          <pre class="system-info-mono">rclone config  # type: webdav, url: {{ webdavEndpoint }}
# macOS Finder：菜单「前往 → 连接服务器」输入端点地址
# Windows：资源管理器「映射网络驱动器」输入端点地址</pre>
        </details>
      </template>
      <p v-else class="system-info-hint">
        WebDAV 未启用。设置 VFILES_WEBDAV_ENABLED=true 并开启认证后可挂载。
      </p>
    </section>

    <!-- 存储概览卡（组件级复用 ✓ 零数据层依赖 ✓ 自取数据） -->
    <section class="system-info-card" aria-label="存储概览">
      <h2 class="system-info-card-title">存储与用量</h2>
      <SidebarOverview />
    </section>

    <!-- 用户统计卡（listUsers total 复用 ✓） -->
    <section class="system-info-card" aria-label="用户统计">
      <h2 class="system-info-card-title">用户</h2>
      <dl class="system-info-list">
        <dt>总数</dt>
        <dd>{{ userTotal ?? "—" }}</dd>
      </dl>
    </section>
    </template>
  </main>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { RouterLink } from "vue-router";
import SidebarOverview from "../components/file-browser/SidebarOverview.vue";
import SkeletonList from "../components/common/SkeletonList.vue";
import { authService } from "../services/auth.service";
import { useAppStore } from "../stores/app.store";

const app = useAppStore();
const info = ref<{
  version: string;
  os: string;
  arch: string;
  uptime_secs: number;
  started_at: string;
  webdav_enabled: boolean;
  webdav_bind: string;
  webdav_embedded: boolean;
  webdav_mount: string;
} | null>(null);
const userTotal = ref<number | null>(null);

const webdavEndpoint = computed(() => {
  // r-new 双模式：嵌入 = 当前页协议/主机 + mount（location.host 含端口 ✗ 0.0.0.0 免疫 ✓）
  if (info.value?.webdav_embedded) {
    const mount = (info.value?.webdav_mount || "/dav").replace(/\/+$/, "");
    return `${window.location.protocol}//${window.location.host}${mount}/`;
  }
  const bind = info.value?.webdav_bind || "";
  const [host, port] = bind.split(":");
  const hostLabel = host === "0.0.0.0" ? window.location.hostname : host;
  return `http://${hostLabel}:${port || ""}/`;
});

const runtimeLabel = computed(() =>
  info.value ? `${info.value.os}/${info.value.arch}` : "—",
);
const uptimeLabel = computed(() => {
  const s = info.value?.uptime_secs ?? 0;
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h} 小时 ${m} 分` : `${m} 分 ${s % 60} 秒`;
});

const loading = ref(false);

async function load() {
  loading.value = true;
  try {
    const [sys, users] = await Promise.all([
      authService.systemInfo(),
      authService.listUsers(),
    ]);
    if (sys.success && sys.data) info.value = sys.data;
    else if (!sys.success) app.error(sys.error || "加载系统信息失败");
    if (users.success && users.data) userTotal.value = users.data.total_count ?? null;
  } finally {
    loading.value = false;
  }
}

onMounted(load);
</script>

<style scoped>
/* 页头布局（admin-header 同式 ✓ 标题左 + 动作右上角 ✓ 用户令「与其他视图一样」） */
.system-info-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
  margin-bottom: 1.2rem;
}

.system-info-header-actions {
  display: flex;
  gap: 0.5rem;
  flex-shrink: 0;
}



.system-info-card {
  margin-bottom: 1.2rem;
  padding: 1.1rem 1.2rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius);
  background: var(--vf-surface-raised);
  box-shadow: var(--vf-shadow-card);
}

.system-info-card-title {
  margin-bottom: 0.8rem;
  color: var(--vf-text-strong);
  font-size: 0.875rem;
  font-weight: 600;
}

.system-info-list {
  display: grid;
  grid-template-columns: 6.5rem 1fr;
  gap: 0.5rem 0.75rem;
  font-size: 0.875rem;
}

.system-info-list dt {
  color: var(--vf-text-muted);
}

.system-info-list dd {
  margin: 0;
  color: var(--vf-text);
  font-variant-numeric: tabular-nums;
}

.system-info-mono {
  font-family: var(--vf-font-mono, monospace);
  word-break: break-all;
}

.system-info-hint {
  margin-top: 0.8rem;
  color: var(--vf-text-muted);
  font-size: 0.8rem;
  line-height: 1.6;
}

.system-info-details {
  margin-top: 0.7rem;
  font-size: 0.8rem;
}

.system-info-details summary {
  cursor: pointer;
  color: var(--vf-accent-text);
}
</style>
