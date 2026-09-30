// ===== 通用类型 =====

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

// ===== 个性化设置类型 =====

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

// ===== 应用初始化类型 =====

export interface PrepareAppData {
  personalization: PersonalizationSettings;
  default_effect: string;
  app_version: string;
  tauri_version: string;
  bootstrap_icons_ready: boolean;
  fonts_ready: boolean;
  icon_ready: boolean;
}

export interface InitAppData {
  personalization: PersonalizationSettings;
  default_effect: string;
  app_version: string;
  tauri_version: string;
}

// ===== 远程页面类型 =====

export interface PageEntry {
  name: string;
  path: string;
  sha256?: string;
}

export interface PageManifest {
  base_url?: string;
  pages: PageEntry[];
}

// ===== 应用更新类型 =====

/**
 * 更新包类型。
 * - `installer`：安装版用（NSIS 安装器，会弹出安装界面）
 * - `portable`：便携版用（单个 exe，直接替换自身）
 * - `portable-zip`：便携整包，仅作为手动下载链接（新用户首次下载）
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
  /** 与当前安装形态匹配、可自动落地的包；为 null 表示只能手动下载 */
  asset: UpdateAsset | null;
  /** 手动下载地址（便携整包 / 安装包），可能为 null */
  manual_url: string | null;
}

/**
 * 构建渠道。它决定这份程序**能不能被自动更新替换**：
 *
 * - `official`：由发布流程产出，可自动更新
 * - `dev`：debug 构建（含 `pnpm tauri dev`），不参与自动更新
 * - `self-built`：用户/第三方自己编译的 release，只提示新版本 + 手动下载，绝不替换文件
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
  /**
   * 是否允许自动替换程序文件。为 false 时 `latest.asset` 必为 null，
   * 界面只能引导用户手动下载。
   */
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

/** 当前运行环境信息（"关于"页用它显示版本形态，无需联网） */
export interface RuntimeInfo {
  /** `portable` | `installed` */
  install_mode: string;
  build_channel: BuildChannel | string;
  /** 当前平台是否支持自动安装（目前仅 Windows） */
  auto_install_supported: boolean;
  /** 当前渠道是否允许自动更新 */
  update_allowed: boolean;
  exe_path: string;
  data_dir: string;
}

/** `update-progress` 事件负载 */
export interface UpdateProgress {
  downloaded: number;
  /** 0 表示总长度未知 */
  total: number;
}

// ===== Terracotta (陶瓦联机) 类型 =====

export interface TerracottaState {
  state: string;
  room?: string;
  url?: string;
  exception_type?: number;
}

// ===== 设置项元配置类型 =====

export type FieldType =
  | "text"
  | "textarea"
  | "number"
  | "switch"
  | "select"
  | "chips"
  | "slider"
  | "password";

export interface FieldOption {
  value: string;
  label: string;
  description?: string;
}

export interface FieldMeta {
  key: string;
  label: string;
  description?: string;
  type: FieldType;
  default: string;
  options: FieldOption[];
  unit: string;
  min?: number;
  max?: number;
  step?: number;
  placeholder: string;
  sensitive: boolean;
  auto_save: boolean;
  group: string;
}

export interface SettingMeta {
  section: string;
  title: string;
  icon: string;
  description: string;
  fields: FieldMeta[];
}

// ===== 设置项清单类型 =====

export interface SettingSection {
  id: string;
  label: string;
  icon: string;
}

export interface SettingManifest {
  sections: SettingSection[];
}
