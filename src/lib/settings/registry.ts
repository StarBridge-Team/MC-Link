import type { Component } from "vue";
import type { SettingSection } from "../api/types";

/**
 * 设置注册表：把「原生分区」与「服务端下发分区」统一成一张表。
 *
 * # 为什么需要它
 *
 * 此前原生分区靠 `SettingView` 里的 `SECTIONS` 硬编码映射，服务端分区又由
 * `SettingsManifestSection` 单独拉取渲染——两套体系互不知道对方，无法做
 * 左树导航、搜索、去重，更谈不上"层级覆盖"。
 *
 * 这里把两者合并成 `SettingEntry[]`：原生分区带组件，服务端分区带元配置 id。
 * `SettingView` 只认这张表，左树、搜索、内容渲染全部由它驱动。
 *
 * # 层级覆盖（B7）
 *
 * `priority` 是层级优先级的落地点：**同 id 冲突时高者胜**。当前唯一的实际语义是
 * 「服务端若下发原生已实现的 id（如 personalization），原生优先、远程跳过」——即下面的去重逻辑。
 * 真正的"跨分区字段继承"（某子分区的同名项覆盖全局）尚无数据模型，先把这个钩子留在这里，
 * 等具体覆盖规则确定后，在 `mergeEntries` 里按 `priority` 做值合并即可，无需改 `SettingView`。
 */
export interface SettingEntry {
  id: string;
  label: string;
  icon: string;
  kind: "native" | "remote";
  /** 原生分区用组件渲染；服务端分区用 `SettingsManifestSection` 按 id 拉元配置。 */
  component?: Component;
  /** 左树分组（如「外观」「常规」）。 */
  group: string;
  /** 层级优先级，越大越优先。 */
  priority: number;
  /** 顶部搜索的匹配词（标题/同义词）。 */
  keywords: string[];
}

import PersonalizationSection from "../../views/settings/PersonalizationSection.vue";
import HmclSection from "../../views/settings/HmclSection.vue";
import HomepageSection from "../../views/settings/HomepageSection.vue";
import GeneralSection from "../../views/settings/GeneralSection.vue";
import UpdateSection from "../../views/settings/UpdateSection.vue";
import AboutSection from "../../views/settings/AboutSection.vue";

/** 原生分区：结构与渲染都在前端，不依赖服务端元配置。顺序即左树默认顺序。 */
export const NATIVE_ENTRIES: SettingEntry[] = [
  {
    id: "personalization",
    label: "个性化",
    icon: "palette",
    kind: "native",
    component: PersonalizationSection,
    group: "外观",
    priority: 100,
    keywords: ["外观", "主题", "颜色", "背景", "语言", "地区", "字体"],
  },
  {
    id: "hmcl-theme",
    label: "HMCL 主题",
    icon: "palette",
    kind: "native",
    component: HmclSection,
    group: "外观",
    priority: 100,
    keywords: ["HMCL", "主题", "配色", "导入", "启动器", "种子色"],
  },
  {
    id: "homepage",
    label: "首页",
    icon: "home",
    kind: "native",
    component: HomepageSection,
    group: "常规",
    priority: 100,
    keywords: ["首页", "公告", "最近游戏", "快捷入口"],
  },
  {
    id: "general",
    label: "通用",
    icon: "tune",
    kind: "native",
    component: GeneralSection,
    group: "常规",
    priority: 100,
    keywords: ["通用", "语言", "地区", "玩家名", "更多设置"],
  },
  {
    id: "update",
    label: "更新",
    icon: "system_update",
    kind: "native",
    component: UpdateSection,
    group: "更新",
    priority: 100,
    keywords: ["更新", "版本", "升级", "发布"],
  },
  {
    id: "about",
    label: "关于",
    icon: "info",
    kind: "native",
    component: AboutSection,
    group: "关于",
    priority: 100,
    keywords: ["关于", "许可", "版本", "第三方", "开源"],
  },
];

const NATIVE_IDS = new Set(NATIVE_ENTRIES.map((e) => e.id));

/**
 * 合并原生与服务端下发分区。
 *
 * 去重：服务端若下发原生已实现的 id（如 `personalization`），跳过——否则同一页会
 * 冒出第二张「个性化」卡片。这就是「层级覆盖」的当前落地：原生优先级高于远程。
 */
export function mergeEntries(remote: SettingSection[]): SettingEntry[] {
  const remoteEntries: SettingEntry[] = remote
    .filter((s) => !NATIVE_IDS.has(s.id))
    .map((s) => ({
      id: s.id,
      label: s.label || s.id,
      icon: s.icon || "tune",
      kind: "remote" as const,
      group: "更多",
      priority: 50,
      keywords: [s.label, s.id],
    }));
  return [...NATIVE_ENTRIES, ...remoteEntries];
}

/** 顶部搜索：匹配标题 / id / 关键词（当前分区级；远程分区的字段级搜索待接入 meta）。 */
export function filterEntries(entries: SettingEntry[], query: string): SettingEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return entries;
  return entries.filter(
    (e) =>
      e.label.toLowerCase().includes(q) ||
      e.id.toLowerCase().includes(q) ||
      e.keywords.some((k) => k.toLowerCase().includes(q)),
  );
}

/** 按 group 分组并保持出现顺序（同组聚合）。 */
export function groupEntries(entries: SettingEntry[]): [string, SettingEntry[]][] {
  const order: string[] = [];
  const map = new Map<string, SettingEntry[]>();
  for (const e of entries) {
    if (!map.has(e.group)) {
      map.set(e.group, []);
      order.push(e.group);
    }
    map.get(e.group)!.push(e);
  }
  return order.map((g) => [g, map.get(g)!]);
}
