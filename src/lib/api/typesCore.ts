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
  /** 配色变体（M3E ThemeVariant）。 */
  theme_variant: string;
  /** 配色对比度（M3E ContrastLevel）。 */
  theme_contrast: string;
  animation_enabled: boolean;
  animation_speed: number;
  transparent_effect: string;
  /**
   * 窗口材质的染色浓度（0–100）：**越高越不透明**。
   *
   * 只对 **Acrylic** 生效；**Mica 调不动**（它的实现没有任何浓度参数）。
   */
  effect_tint: number;
  background_type: string;
  background_value: string;
  background_fit: string;
  background_overlay: boolean;
  background_overlay_opacity: number;
  /**
   * 页面背景不透明度（0–100）：**越高越遮住底下的材质**。
   *
   *   100 → 完全遮住材质；0 → 材质完全显现；50 → 半显现。
   */
  background_opacity: number;
  /** 背景不透明度的深色档（`background_opacity` 是浅色档）。 */
  background_opacity_dark: number;
  /** 图片背景模糊强度（px），分深浅两档。 */
  background_image_blur_light: number;
  background_image_blur_dark: number;
  /** 视频背景模糊强度（px），分深浅两档。 */
  background_video_blur_light: number;
  background_video_blur_dark: number;
  /** 种子色（材质）背景模糊强度（px），分深浅两档。 */
  seed_blur_light: number;
  seed_blur_dark: number;
  /** 遮罩强度深色档（`background_overlay_opacity` 是浅色档）。 */
  background_overlay_opacity_dark: number;
  music_mode: string;
  music_value: string;
  homepage_mode: string;
  homepage_value: string;
  /**
   * 配色种子是否取自背景图（"配色跟随背景图"）。
   *
   * 开启后 `theme_color` 退居为**回退值**：取色失败时仍用它，避免界面失去主题色。
   */
  theme_from_background: boolean;
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

/**
 * 只读沙箱信息（目前仅 Flatpak）。
 *
 * 出现它意味着应用装在只读位置、由外部包管理器管理：**应用内更新在机制上
 * 不可能成功**，界面必须改为引导用户执行 `update_command`。
 */
export interface SandboxInfo {
  /** 沙箱类型标识，目前恒为 `flatpak`。 */
  kind: string;
  /** 建议用户执行的更新命令，如 `flatpak update`。 */
  update_command: string;
}

/** 当前运行环境（"关于"页用它显示版本形态，无需联网）。 */
export interface RuntimeInfo {
  /** `portable` | `installed` */
  install_mode: string;
  build_channel: BuildChannel | string;
  /**
   * 当前环境是否支持自动安装。
   *
   * 后端已把 Flatpak 收敛为 false（`is_auto_install_supported`），
   * 因此界面只需看这一个字段，不必自己判断沙箱。
   */
  auto_install_supported: boolean;
  /** 当前渠道是否允许自动更新 */
  update_allowed: boolean;
  exe_path: string;
  data_dir: string;
  /** 只读沙箱信息；缺省表示常规安装形态。 */
  sandbox?: SandboxInfo | null;
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
