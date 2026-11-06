import type { Component } from "vue";
import type { SettingSection } from "../api/types";

import PersonalizationSection from "../../views/settings/PersonalizationSection.vue";
import HmclSection from "../../views/settings/HmclSection.vue";
import HomepageSection from "../../views/settings/HomepageSection.vue";
import GeneralSection from "../../views/settings/GeneralSection.vue";
import UpdateSection from "../../views/settings/UpdateSection.vue";
import AboutSection from "../../views/settings/AboutSection.vue";

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
  /**
   * 搜索关键词。
   *
   * **要把该分区里每个设置项的名字都写进来**——搜索框匹配的是"设置项"，
   * 不是"分区标题"。用户搜「模糊」「对比度」时，得能定位到「个性化」。
   */
  keywords: string[];
}

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
    keywords: [
      "外观",
      "主题",
      "主题色",
      "配色",
      "颜色",
      "明暗",
      "深色",
      "浅色",
      "对比度",
      "变体",
      "窗口材质",
      "材质",
      "mica",
      "亚克力",
      "acrylic",
      "动画",
      "动画速度",
      "缩放",
      "界面缩放",
      "音乐音量",
      "背景",
      "背景图",
      "背景视频",
      "背景音乐",
      "模糊",
      "透明度",
      "遮罩",
      "不透明度",
      "种子色",
      "语言",
      "地区",
      "字体",
    ],
  },
  {
    id: "hmcl-theme",
    label: "HMCL 主题",
    icon: "palette",
    kind: "native",
    component: HmclSection,
    group: "外观",
    priority: 100,
    keywords: ["HMCL", "主题", "主题包", "导入", "配色", "种子色", "启动器", "皮肤"],
  },
  {
    id: "homepage",
    label: "首页",
    icon: "home",
    kind: "native",
    component: HomepageSection,
    group: "常规",
    priority: 100,
    keywords: ["首页", "公告", "横幅", "最近游戏", "快捷入口", "布局", "模式"],
  },
  {
    id: "general",
    label: "通用",
    icon: "tune",
    kind: "native",
    component: GeneralSection,
    group: "常规",
    priority: 100,
    keywords: ["通用", "玩家名", "昵称", "离线", "UUID", "账号", "语言", "地区", "更多设置"],
  },
  {
    id: "update",
    label: "更新",
    icon: "system_update",
    kind: "native",
    component: UpdateSection,
    group: "更新",
    priority: 100,
    keywords: ["更新", "版本", "升级", "检查更新", "自动更新", "构建渠道", "发布"],
  },
  {
    id: "about",
    label: "关于",
    icon: "info",
    kind: "native",
    component: AboutSection,
    group: "关于",
    priority: 100,
    keywords: ["关于", "版本", "开源", "许可", "许可证", "第三方", "致谢", "社区", "反馈", "问题"],
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

/**
 * 顶部搜索：匹配分区标题 / id / 关键词，并可下探到**分区内的字段名**。
 *
 * `fieldsOf` 由调用方提供（远程分区的字段来自 `get_setting_meta`）。原生分区的
 * 字段名写在 `keywords` 里——它们的界面是固定组件，没有可读取的元配置。
 */
export function filterEntries(
  entries: SettingEntry[],
  query: string,
  fieldsOf?: (id: string) => string[],
): SettingEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return entries;
  const hit = (value: string) => value.toLowerCase().includes(q);
  return entries.filter(
    (e) =>
      hit(e.label) ||
      hit(e.id) ||
      e.keywords.some(hit) ||
      (fieldsOf?.(e.id) ?? []).some(hit),
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
