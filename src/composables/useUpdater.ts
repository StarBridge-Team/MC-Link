import { computed, onScopeDispose, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  checkUpdate,
  clearUpdateCache,
  downloadUpdate,
  getRuntimeInfo,
  installUpdate,
} from "../lib/api/update";
import type {
  BuildChannel,
  RuntimeInfo,
  UpdateInfo,
  UpdateProgress,
} from "../lib/api/types";

/**
 * 自动更新流程。
 *
 * # 两道闸门（决定这份程序能不能被自动更新）
 *
 * 1. **构建渠道**：只有发布流程产出的 `official` 构建允许自动更新。
 *    开发构建（`dev`）连检查都不会发起；自行构建（`self-built`）只提示新版本 + 手动下载。
 *    判定在后端（`src-tauri/src/build_channel.rs`），前端只负责据此显示正确的文案。
 * 2. **安装形态**：便携版替换自身 exe，安装版交给安装器；由后端按 `latest.asset` 决定，
 *    前端不需要区分。
 *
 * 前端的判断依据就是 `updateAllowed` 与 `canAutoInstall` 两个标志。
 *
 * # 典型用法
 *
 * ```ts
 * const updater = useUpdater();
 * await updater.loadRuntimeInfo();     // 进入关于页即可调用，不联网
 * await updater.check();               // 用户点"检查更新"时
 * if (updater.hasUpdate.value) {
 *   if (updater.canAutoInstall.value) {
 *     await updater.download();        // 可选：让用户看到进度
 *     await updater.install();         // 应用会在约 0.6 秒后退出并重启
 *   } else {
 *     // 用 updater.manualUrl.value 打开浏览器，或按 updater.blockedReason.value 显示原因
 *   }
 * }
 * ```
 *
 * 强制更新时（`latest.mandatory`）界面不应提供"稍后再说"。
 */
/**
 * 不能自动更新的原因代号（不是给人看的文案）。
 *
 * `dev` / `self-built` 由构建渠道推导；`unsupported` 表示渠道允许但当前形态无对应包；
 * `no-asset` 表示清单里没有匹配本安装形态的包。界面负责把代号翻成 `update.*` 文案；
 * 若收到的是后端原文（例如下载失败原因），直接展示即可。
 */
export type UpdateBlockReason = "dev" | "self-built" | "unsupported" | "no-asset";

export function useUpdater() {
  const checking = ref(false);
  const downloading = ref(false);
  const installing = ref(false);

  const hasUpdate = ref(false);
  const currentVersion = ref("");
  const latest = ref<UpdateInfo | null>(null);
  /** `portable` | `installed` */
  const installMode = ref("");
  /** `dev` | `official` | `self-built` */
  const buildChannel = ref<BuildChannel | string>("");
  /** 当前构建是否允许自动替换程序文件 */
  const updateAllowed = ref(false);

  /** 进度百分比；长度未知时为 -1 */
  const progress = ref(0);
  const downloaded = ref(0);
  const total = ref(0);

  const error = ref<string | null>(null);

  /** 当前是否为开发构建（界面可据此隐藏"检查更新"入口） */
  const isDevBuild = computed(() => buildChannel.value === "dev");
  /** 当前是否为自行构建的版本 */
  const isSelfBuilt = computed(() => buildChannel.value === "self-built");
  /** 是否允许自动安装：渠道许可 + 存在对应形态的更新包 */
  const canAutoInstall = computed(
    () => updateAllowed.value && latest.value?.asset != null,
  );
  /** 手动下载地址 */
  const manualUrl = computed(() => latest.value?.manual_url ?? null);
  /** 当前是否是便携版（界面可据此说明"将替换程序文件"） */
  const isPortable = computed(() => installMode.value === "portable");
  /** 是否强制更新 */
  const mandatory = computed(() => latest.value?.mandatory === true);

  /**
   * 不能自动更新的原因代号（界面据此取文案，不在 composable 里写死中文）。
   * 文案在 `i18n` 的 `update.blockedDev` / `update.blockedSelfBuilt` / `update.noAsset`。
   */
  const blockedReason = computed<UpdateBlockReason | null>(() => {
    if (buildChannel.value === "dev") return "dev";
    if (buildChannel.value === "self-built") return "self-built";
    return null;
  });

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

  /**
   * 检查更新。
   *
   * 开发构建会直接返回"无更新"（后端不会发起网络请求），此时 `updateAllowed` 为 false、
   * `latest` 为 null——界面应显示构建渠道说明，而不是"已是最新版本"。
   */
  async function check() {
    checking.value = true;
    error.value = null;
    try {
      const result = await checkUpdate();
      hasUpdate.value = result.has_update;
      currentVersion.value = result.current_version;
      installMode.value = result.install_mode;
      buildChannel.value = result.build_channel;
      updateAllowed.value = result.update_allowed;
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
   * 当前构建不允许自动更新时**不会发起调用**，直接抛出可展示的原因。
   */
  async function install() {
    if (!updateAllowed.value) {
      const reason: UpdateBlockReason = blockedReason.value ?? "unsupported";
      error.value = reason;
      throw new Error(reason);
    }

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

  /**
   * 查询当前运行环境（不联网）。
   *
   * "关于"页首次渲染即可调用：安装形态、构建渠道、程序路径、数据目录。
   */
  async function loadRuntimeInfo(): Promise<RuntimeInfo> {
    const info = await getRuntimeInfo();
    installMode.value = info.install_mode;
    buildChannel.value = info.build_channel;
    updateAllowed.value = info.update_allowed;
    return info;
  }

  function requireAsset() {
    const asset = latest.value?.asset;
    if (!asset) {
      const reason: UpdateBlockReason = blockedReason.value ?? "no-asset";
      error.value = reason;
      throw new Error(reason);
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
    buildChannel,
    updateAllowed,
    progress,
    downloaded,
    total,
    error,
    // 派生
    isDevBuild,
    isSelfBuilt,
    canAutoInstall,
    manualUrl,
    isPortable,
    mandatory,
    blockedReason,
    // 动作
    check,
    download,
    install,
    clearCache,
    loadRuntimeInfo,
  };
}

function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
