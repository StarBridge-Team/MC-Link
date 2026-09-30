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

export interface UpdateInfo {
  version: string;
  download_url: string;
  release_notes: string;
  release_date: string;
  mandatory: boolean;
}

export interface CheckUpdateResult {
  has_update: boolean;
  current_version: string;
  latest?: UpdateInfo;
}

export interface DownloadUpdateResult {
  success: boolean;
  path?: string;
  message: string;
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
