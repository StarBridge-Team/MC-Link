import { computed, onScopeDispose, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  checkUpdate,
  clearUpdateCache,
  downloadUpdate,
  getInstallMode,
  installUpdate,
} from "../lib/api/update";
import type { InstallModeInfo, UpdateInfo, UpdateProgress } from "../lib/api/types";

/**
 * 自动更新流程。
 *
 * # 两种安装形态都由后端处理
 *
 * - **便携版**（exe 同目录有 `data/` 或 `portable.txt`）：下载新 exe，直接替换自身，随后重启；
 * - **安装版**：下载 NSIS 安装器并运行，装完重新拉起应用。
 *
 * 前端不需要区分这两条路径——`check()` 返回的 `latest.asset` 已经是对应形态的包，
 * `install()` 会把它交给后端。前端只需要处理"有没有自动更新包"：
 * `latest.asset === null` 时说明该形态没有可用更新包，应引导用户打开 `manual_url` 手动下载。
 *
 * # 典型用法
 *
 * ```ts
 * const updater = useUpdater();
 * await updater.check();               // 进入关于页时
 * if (updater.hasUpdate.value) {
 *   if (updater.canAutoInstall.value) {
 *     await updater.download();        // 可选：让用户看到进度
 *     await updater.install();         // 应用会在约 0.6 秒后退出并重启
 *   } else {
 *     // 用 updater.manualUrl.value 打开浏览器 / 让用户手动下载
 *   }
 * }
 * ```
 *
 * 强制更新时（`latest.mandatory`）界面不应提供"稍后再说"。
 */
export function useUpdater() {
  const checking = ref(false);
  const downloading = ref(false);
  const installing = ref(false);

  const hasUpdate = ref(false);
  const currentVersion = ref("");
  const latest = ref<UpdateInfo | null>(null);
  /** `portable` | `installed` */
  const installMode = ref("");

  /** 进度百分比；长度未知时为 -1 */
  const progress = ref(0);
  const downloaded = ref(0);
  const total = ref(0);

  const error = ref<string | null>(null);

  /** 是否存在可自动安装的更新包（false 时只能手动下载） */
  const canAutoInstall = computed(() => latest.value?.asset != null);
  /** 手动下载地址 */
  const manualUrl = computed(() => latest.value?.manual_url ?? null);
  /** 当前是否是便携版（界面可据此说明"将替换程序文件"） */
  const isPortable = computed(() => installMode.value === "portable");
  /** 是否强制更新 */
  const mandatory = computed(() => latest.value?.mandatory === true);

  let unlisten: UnlistenFn | null = null;

  /** 订阅下载进度事件（只订阅一次）。 */
  async function bindProgress() {
    if (unlisten) return;
    unlisten = await listen<UpdateProgress>("update-progress", (event) => {
      const { downloaded: done, total: size } = event.payload;
      downloaded.value = done;
      total.value = size;
      progress.value = size > 0 ? Math.min(100, Math.round((done / size) * 100)) : -1;
    });
  }

  /** 检查更新。 */
  async function check() {
    checking.value = true;
    error.value = null;
    try {
      const result = await checkUpdate();
      hasUpdate.value = result.has_update;
      currentVersion.value = result.current_version;
      installMode.value = result.install_mode;
      latest.value = result.latest;
      return result;
    } catch (e) {
      error.value = messageOf(e);
      throw e;
    } finally {
      checking.value = false;
    }
  }

  /** 下载更新包（不安装）。`install()` 内部也会按需下载，因此这一步是可选的。 */
  async function download() {
    const asset = requireAsset();
    downloading.value = true;
    error.value = null;
    progress.value = 0;
    downloaded.value = 0;
    total.value = 0;
    try {
      await bindProgress();
      return await downloadUpdate(asset);
    } catch (e) {
      error.value = messageOf(e);
      throw e;
    } finally {
      downloading.value = false;
    }
  }

  /**
   * 安装更新并重启应用。
   *
   * 调用前请先给用户提示：成功后应用会在约 0.6 秒内退出。
   */
  async function install() {
    const asset = requireAsset();
    installing.value = true;
    error.value = null;
    try {
      return await installUpdate(asset);
    } catch (e) {
      error.value = messageOf(e);
      installing.value = false;
      throw e;
    }
    // 成功时不复位 installing：应用即将退出，界面停留在"正在重启"更合理
  }

  /** 清理更新缓存。 */
  async function clearCache() {
    await clearUpdateCache();
    downloaded.value = 0;
    total.value = 0;
    progress.value = 0;
  }

  /** 仅查询安装形态（不联网），用于"关于"页显示当前形态与程序路径。 */
  async function loadInstallMode(): Promise<InstallModeInfo> {
    const info = await getInstallMode();
    installMode.value = info.install_mode;
    return info;
  }

  function requireAsset() {
    const asset = latest.value?.asset;
    if (!asset) {
      const msg = "该版本未提供适配当前安装形态的更新包，请手动下载安装包";
      error.value = msg;
      throw new Error(msg);
    }
    return asset;
  }

  onScopeDispose(() => {
    unlisten?.();
    unlisten = null;
  });

  return {
    // 状态
    checking,
    downloading,
    installing,
    hasUpdate,
    currentVersion,
    latest,
    installMode,
    progress,
    downloaded,
    total,
    error,
    // 派生
    canAutoInstall,
    manualUrl,
    isPortable,
    mandatory,
    // 动作
    check,
    download,
    install,
    clearCache,
    loadInstallMode,
  };
}

function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
