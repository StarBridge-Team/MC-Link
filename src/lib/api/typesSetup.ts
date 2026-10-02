// 后端契约的类型镜像（设置元配置 + 首次启动引导 / 法务 / 插件路由推荐）。
//
// 详见 `typesCore.ts` 顶部的说明。

// ===== 设置项元配置（后端 setting_meta.rs） =====

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

export interface SettingSection {
  id: string;
  label: string;
  icon: string;
}

export interface SettingManifest {
  sections: SettingSection[];
}

// ===== 首次启动引导（OOBE，后端 setup.rs / legal.rs） =====

/** 已完成同意记录；`unfetched` 表示当时拉不到条款正文。 */
export interface EulaConsent {
  version: string;
  sha256: string;
  accepted_at: string;
}

/** 三步是否已满足；由状态派生，后端不存"当前第几步"。 */
export interface SetupSteps {
  language: boolean;
  eula: boolean;
  game: boolean;
}

/**
 * 引导状态。
 *
 * `language` / `region` 是**当前生效**值；`detected_*` 供引导页预选；
 * `supported_*` 是可选清单——界面不要硬编码这些选项，以后端返回为准。
 */
export interface SetupState {
  completed: boolean;
  language: string;
  region: string;
  detected_language: string;
  detected_region: string;
  supported_languages: string[];
  supported_regions: string[];
  steps: SetupSteps;
  eula_accepted: EulaConsent | null;
  game: string;
  /** 开发构建跳过引导（后端 debug_assertions && !test）。 */
  dev_skip: boolean;
}

export interface LegalDocView {
  id: string;
  version: string;
  title: string;
  requires_acceptance: boolean;
  text: string;
  sha256: string;
}

export interface LegalBundle {
  language: string;
  /** 拉不到条款时为 null，此时只能走 `fallback_url`。 */
  eula: LegalDocView | null;
  attachments: LegalDocView[];
  accepted: EulaConsent | null;
  /** 条款版本或哈希变化时为 true，需要重新同意。 */
  needs_consent: boolean;
  /** 官网兜底链接（条款拉不到时展示）。 */
  fallback_url: string;
  error: string | null;
}

// ===== 插件路由推荐（OOBE 选游戏与插件管理界面共用同一引擎） =====

export interface RouteCandidate {
  pluginId: string;
  reason: string;
  score: number;
}

export interface RoutePlan {
  primary: string | null;
  candidates: RouteCandidate[];
}

export interface GameRecommendation {
  gameId: string;
  gameName: string;
  aliases: string[];
  requiresCoupler: boolean;
  recommendations: {
    adapter: RoutePlan;
    detector: RoutePlan;
    coupler: RoutePlan;
  };
}
