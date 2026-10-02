// 后端契约的类型镜像（基础域）。
//
// 所有字段名必须与 `src-tauri/src/**` 的 serde 输出逐字一致：这些结构会直接
// 序列化进 yml / JSON 或跨 IPC 传递，改名等于让老配置失效或让字段静默读不到。
// 后端权威定义位置见每个类型上方的注释。
//
// 本目录按域拆成三个文件（core / setup / plugin），由 `types.ts` 统一出口，
// 这样调用方仍然只 import `lib/api/types`，而单个文件不超行数上限。

// ===== 通用 =====

export interface IpInfo {
  region: string;
  isp: string;
}

export interface AdapterStatus {
  installed: boolean;
  running: boolean;
  starting: boolean;
  port: number | null;
}

// ===== 个性化设置（后端 config/mod.rs::PersonalizationSettings） =====
//
// 字段名即 `Setting/personalization.yml` 的 key，**冻结契约**。

export interface PersonalizationSettings {
  theme_color: string;
  theme_mode: string;
  animation_enabled: boolean;
  animation_speed: number;
  transparent_effect: string;
  background_type: string;
  background_value: string;
  background_fit: string;
  background_overlay: boolean;
  background_overlay_opacity: number;
  music_mode: string;
  music_value: string;
  homepage_mode: string;
  homepage_value: string;
}

export interface BackgroundFile {
  name: string;
  is_video: boolean;
}

// ===== 应用初始化 =====

/** 单个远程资源的就绪状态（清单驱动，不逐个硬编码）。 */
export interface AssetState {
  path: string;
  ready: boolean;
  reason?: string;
}

export interface PrepareAppData {
  personalization: PersonalizationSettings;
  default_effect: string;
  app_version: string;
  tauri_version: string;
  /** 资源清单版本（诊断用）。 */
  asset_version: string;
  /** 是否全部远程资源就绪。 */
  assets_ready: boolean;
  /** 每个资源的就绪状态，界面据此决定注入哪些 CSS。 */
  assets: AssetState[];
  /** 失败原因，可直接展示。 */
  asset_failures: string[];
  /** 是否处于离线降级（未连上资源服务器，仅用本地缓存判定）。 */
  assets_offline: boolean;
}

export interface InitAppData {
  personalization: PersonalizationSettings;
  default_effect: string;
  app_version: string;
  tauri_version: string;
}

// ===== 应用更新（后端 update/model.rs） =====

/**
 * 更新包类型。
 * - `installer`：安装版用（NSIS 安装器）
 * - `portable`：便携版用（单个 exe，直接替换自身）
 * - `portable-zip`：便携整包，仅作为手动下载链接
 */
export type UpdateAssetKind = "installer" | "portable" | "portable-zip";

export interface UpdateAsset {
  /** 形如 `windows-x86_64` */
  platform: string;
  kind: UpdateAssetKind | string;
  /** 文件名，同时是缓存的落地名 */
  file: string;
  /** 候选下载地址，按顺序尝试 */
  urls: string[];
  sha256: string;
  size: number | null;
}

export interface UpdateInfo {
  version: string;
  release_notes: string;
  release_date: string;
  /** 强制更新：界面不应提供"稍后再说" */
  mandatory: boolean;
  /** 与当前安装形态匹配、可自动落地的包；null 表示只能手动下载 */
  asset: UpdateAsset | null;
  /** 手动下载地址，可能为 null */
  manual_url: string | null;
}

/**
 * 构建渠道，决定这份程序**能不能被自动更新替换**：
 * - `official`：由发布流程产出，可自动更新
 * - `dev`：debug 构建，不参与自动更新（连检查都不发起）
 * - `self-built`：自行编译的 release，只提示新版本 + 手动下载
 */
export type BuildChannel = "dev" | "official" | "self-built";

export interface CheckUpdateResult {
  has_update: boolean;
  /** 形如 `v0.4.0` */
  current_version: string;
  /** `portable` | `installed` */
  install_mode: string;
  /** 形如 `windows-x86_64` */
  platform: string;
  build_channel: BuildChannel | string;
  /** 是否允许自动替换程序文件；false 时 `latest.asset` 必为 null */
  update_allowed: boolean;
  latest: UpdateInfo | null;
}

export interface DownloadUpdateResult {
  success: boolean;
  /** 缓存目录内的落地文件名 */
  file: string;
  size: number;
  message: string;
}

export interface InstallUpdateResult {
  /** 恒为 true：落地流程已启动，应用随即退出并重启 */
  restarting: boolean;
  message: string;
}

/** 当前运行环境（"关于"页用它显示版本形态，无需联网）。 */
export interface RuntimeInfo {
  /** `portable` | `installed` */
  install_mode: string;
  build_channel: BuildChannel | string;
  /** 当前平台是否支持自动安装 */
  auto_install_supported: boolean;
  /** 当前渠道是否允许自动更新 */
  update_allowed: boolean;
  exe_path: string;
  data_dir: string;
}

/** `update-progress` 事件负载。 */
export interface UpdateProgress {
  downloaded: number;
  /** 0 表示总长度未知 */
  total: number;
}

// ===== 陶瓦适配器 =====

export interface TerracottaState {
  state: string;
  room?: string;
  url?: string;
  exception_type?: number;
}

// ===== 窗口 / 托盘 =====

/** `tray-resize` 事件负载。 */
export interface TrayResizePayload {
  width: number;
  height: number;
}

/** `deep-link` 事件负载（`mclink://<action>/<param>/...`）。 */
export interface DeepLinkPayload {
  action: string;
  params: string[];
}
