<template>
  <section class="ftp-import-hint">
    <button
      class="ftp-import-toggle"
      type="button"
      :aria-expanded="expanded ? 'true' : 'false'"
      @click="toggle"
    >
      <IconServer :size="18" class="ftp-import-toggle-icon" />
      <span class="ftp-import-toggle-text">用 FTP 批量导入</span>
      <span class="ftp-import-toggle-hint">适合成百上千个文件</span>
      <IconChevronDown
        :size="16"
        class="ftp-import-caret"
        :class="{ 'is-open': expanded }"
      />
    </button>

    <div v-if="expanded" class="ftp-import-body">
      <div v-if="loading" class="ftp-import-state">
        <div class="spinner mb-2"></div>
        <p>正在读取 FTP 连接信息…</p>
      </div>

      <div v-else-if="error" class="notification is-warning is-light mb-0">
        {{ error }}
      </div>

      <template v-else-if="info && info.enabled">
        <dl class="ftp-import-facts">
          <div class="ftp-import-fact">
            <dt>
              服务器<span class="ftp-import-source">{{ hostSourceLabel }}</span>
            </dt>
            <dd>
              <code class="ftp-import-value" :title="info.host">
                {{ info.host }}
              </code>
              <button
                class="vf-ghost-button ftp-import-copy"
                type="button"
                title="复制服务器地址"
                @click="copy('host', info.host)"
              >
                <IconCopy :size="14" />
                <span>{{ copiedKey === "host" ? "已复制" : "复制" }}</span>
              </button>
            </dd>
          </div>
          <div class="ftp-import-fact">
            <dt>端口</dt>
            <dd>
              <code class="ftp-import-value">{{ info.port }}</code>
            </dd>
          </div>
          <div class="ftp-import-fact">
            <dt>账号</dt>
            <dd>
              <code class="ftp-import-value">{{
                username || "当前登录账号"
              }}</code>
            </dd>
          </div>
          <div class="ftp-import-fact">
            <dt>传输加密</dt>
            <dd>
              <span v-if="info.tls.required" class="tag is-success is-light">
                必须使用 FTPS
              </span>
              <span v-else-if="info.tls.enabled" class="tag is-info is-light">
                支持 FTPS
              </span>
              <span v-else class="tag is-warning is-light">
                明文（仅建议内网）
              </span>
            </dd>
          </div>
        </dl>

        <p
          v-if="info.remote_reachable === false"
          class="ftp-import-warning"
          role="note"
        >
          <IconAlertTriangle :size="16" class="ftp-import-warning-icon" />
          <span>
            上面是仅本机可访问的地址；其它机器上的客户端请改用服务器的内网/公网
            IP 或域名，也可让管理员设置
            <code>VFILES_FTP_PASSIVE_HOST</code> 指定对外地址。
          </span>
        </p>

        <p class="ftp-import-note">
          登录后 <code>/</code> 就是你的文件根目录，上传到
          <code>{{ targetPath || "/" }}</code>
          即当前目录；同名文件会生成新版本。
        </p>

        <div v-if="info.passive_ports" class="ftp-import-note">
          被动模式端口范围
          <code
            >{{ info.passive_ports.start }}-{{ info.passive_ports.end }}</code
          >
          ，客户端与防火墙需放行。
        </div>

        <div v-if="info.example_command" class="ftp-import-command">
          <code class="ftp-import-command-text">{{
            info.example_command
          }}</code>
          <button
            class="vf-ghost-button ftp-import-copy"
            type="button"
            title="复制示例命令"
            @click="copy('command', info.example_command)"
          >
            <IconCopy :size="14" />
            <span>{{ copiedKey === "command" ? "已复制" : "复制" }}</span>
          </button>
        </div>

        <p class="ftp-import-note ftp-import-note-muted">
          支持 FileZilla / WinSCP / <code>lftp</code> /
          <code>curl</code> 等客户端， 可直接递归上传整个目录。
        </p>
      </template>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  IconAlertTriangle,
  IconChevronDown,
  IconCopy,
  IconServer,
} from "@tabler/icons-vue";
import { filesService } from "../../services/files.service";
import { useAuthStore } from "../../stores/auth.store";
import { copyText } from "../../utils/clipboard";

type PassivePorts = { start: number; end: number };
type FtpHostSource =
  | "passive_host"
  | "request"
  | "public_base_url"
  | "bind_address"
  | "detected_address"
  | "loopback";

export type FtpConnectionInfo = {
  enabled: boolean;
  host: string;
  host_source?: FtpHostSource;
  remote_reachable?: boolean;
  port: number;
  passive_ports?: PassivePorts;
  tls: { enabled: boolean; required: boolean };
  example_command?: string | null;
  path_mapping?: string;
};

const props = defineProps<{
  /** 上传目标目录（展示用）。 */
  targetPath?: string;
}>();

const auth = useAuthStore();
const expanded = ref(false);
const loading = ref(false);
const error = ref<string | null>(null);
const info = ref<FtpConnectionInfo | null>(null);
/** 最近一次复制成功的按钮标识，用于就地反馈。 */
const copiedKey = ref<string | null>(null);

const username = computed(() => auth.user?.username ?? "");

const HOST_SOURCE_LABELS: Record<FtpHostSource, string> = {
  passive_host: "管理员指定",
  request: "当前访问地址",
  public_base_url: "站点地址",
  bind_address: "服务绑定地址",
  detected_address: "本机网卡地址",
  loopback: "本机回环",
};

/** 地址来源说明：让用户知道这个地址是怎么来的，避免误用 localhost。 */
const hostSourceLabel = computed(() => {
  const source = info.value?.host_source;
  return source ? (HOST_SOURCE_LABELS[source] ?? "") : "";
});

async function loadInfo() {
  if (info.value || loading.value) return;
  loading.value = true;
  error.value = null;

  try {
    info.value = await filesService.getFtpInfo();
  } catch (err) {
    error.value =
      err instanceof Error ? err.message : "暂时无法获取 FTP 连接信息";
  } finally {
    loading.value = false;
  }
}

function toggle() {
  expanded.value = !expanded.value;
  if (expanded.value) void loadInfo();
}

async function copy(key: string, value: string | null | undefined) {
  if (!value) return;
  const ok = await copyText(value);
  if (!ok) return;
  copiedKey.value = key;
  window.setTimeout(() => {
    if (copiedKey.value === key) copiedKey.value = null;
  }, 1500);
}

// 目标目录变化时重新展示（连接信息本身不变，但提示中的路径要跟着更新）
watch(
  () => props.targetPath,
  () => {
    copiedKey.value = null;
  },
);
</script>

<style scoped>
.ftp-import-hint {
  margin-top: 0.75rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius);
  background: var(--vf-surface-sunken);
}

.ftp-import-toggle {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  width: 100%;
  padding: 0.6rem 0.75rem;
  border: none;
  background: transparent;
  color: var(--vf-text);
  font-size: 0.875rem;
  font-weight: 600;
  text-align: left;
  cursor: pointer;
}

.ftp-import-toggle-icon {
  color: var(--vf-accent-text);
  flex: 0 0 auto;
}

.ftp-import-toggle-text {
  flex: 0 0 auto;
}

.ftp-import-toggle-hint {
  flex: 1 1 auto;
  color: var(--vf-text-subtle);
  font-size: 0.8rem;
  font-weight: 400;
}

.ftp-import-caret {
  color: var(--vf-text-muted);
  transition: transform 0.15s var(--vf-motion-standard);
}

.ftp-import-caret.is-open {
  transform: rotate(180deg);
}

.ftp-import-body {
  padding: 0 0.75rem 0.75rem;
}

.ftp-import-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 0.75rem;
  color: var(--vf-text-muted);
  font-size: 0.875rem;
}

.ftp-import-facts {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(11rem, 1fr));
  gap: 0.4rem 0.75rem;
  margin: 0 0 0.6rem;
}

.ftp-import-fact {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
  min-width: 0;
}

.ftp-import-fact dt {
  color: var(--vf-text-subtle);
  font-size: 0.75rem;
}

.ftp-import-fact dd {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  margin: 0;
  min-width: 0;
  font-size: 0.875rem;
}

.ftp-import-value {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.8rem;
}

.ftp-import-copy {
  flex: 0 0 auto;
  min-height: 1.6rem;
  padding: 0 0.4rem;
  font-size: 0.75rem;
}

.ftp-import-source {
  margin-left: 0.3rem;
  padding: 0 0.25rem;
  border-radius: var(--vf-radius-xs);
  background: var(--vf-surface);
  color: var(--vf-text-subtle);
  font-size: 0.68rem;
  font-weight: 400;
}

.ftp-import-warning {
  display: flex;
  align-items: flex-start;
  gap: 0.35rem;
  margin: 0 0 0.5rem;
  padding: 0.45rem 0.55rem;
  border: 1px solid var(--vf-warning-line);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-warning-soft);
  color: var(--vf-text);
  font-size: 0.8rem;
  line-height: 1.5;
}

.ftp-import-warning-icon {
  flex: 0 0 auto;
  margin-top: 0.1rem;
  color: var(--vf-warning-text);
}

.ftp-import-note {
  margin: 0 0 0.4rem;
  color: var(--vf-text-muted);
  font-size: 0.8rem;
  line-height: 1.5;
}

.ftp-import-note-muted {
  margin-bottom: 0;
  color: var(--vf-text-subtle);
}

.ftp-import-command {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  margin-bottom: 0.5rem;
  padding: 0.4rem 0.5rem;
  border: 1px solid var(--vf-border-weak);
  border-radius: var(--vf-radius-sm);
  background: var(--vf-surface);
}

.ftp-import-command-text {
  flex: 1 1 auto;
  min-width: 0;
  overflow-x: auto;
  white-space: nowrap;
  font-size: 0.75rem;
}

/* reduced-motion：transform/width/all 过渡含位移或布局动画，降级为瞬时
   （色/透明/阴影类淡入不在此列 = 无位移风险）。 */
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
