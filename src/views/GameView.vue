<script setup lang="ts">
import { computed, onMounted, ref, watch, type Component } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import GamesSection from "./game/GamesSection.vue";
import MarketSection from "./game/MarketSection.vue";
import PluginsSection from "./game/PluginsSection.vue";
import { useGameSearch } from "../composables/useGameSearch";
import { DEFAULT_GAME_TAB, GAME_TABS, isGameTab, type GameTab } from "../router";

/**
 * 「游戏」页：市场 / 游戏 / 插件 三个模式共用一套外壳。
 *
 * 工具栏固定在内容区顶部 —— **左边搜索框，右边 Tab**。搜索框的位置与外观不随 Tab 变化，
 * 变的只是它绑定的搜索词（每个模式各记各的，见 `useGameSearch`）。
 *
 * # Tab 条：`<m3e-nav-bar mode="expanded">`
 *
 * `@m3e/web` 的 `<m3e-tabs>` 把 Tab 头渲染在自己的 shadow 里，没法把搜索框放在同一行的
 * 左边（也塞不进任何插槽）；`<m3e-nav-bar mode="expanded">` 则**只有条目、没有面板**，
 * 条目横向排（icon + label），选中指示器是 `secondary-container` 胶囊 —— 正好能像搜索框
 * 那样并排放在工具栏里，观感也正是 M3 的选择器。选中态**声明式绑定**（`:selected`）由路由
 * 驱动，与左侧 `NavRail` 同一套写法，避免"点了又变灰"。
 *
 * # 切换动画：左右滑动翻页 + 淡入
 *
 * 用 Vue 内置 `<Transition>`（只动 `transform`/`opacity`，不写 `@keyframes`，不手搓
 * translateX 轨道）。内容按 `tab` 作 `:key` 重挂载，所以每次切换都会重新播放各分区的
 * 逐项错峰入场（`.stagger`）；`appear` 让首次进入「游戏」页也有淡入。进/出场期间两个分区
 * 都绝对定位叠放，是真正的整页翻动，不会再出现之前分屏 `v-if` 延迟挂载导致的「先滑到空白
 * 格再 pop」闪屏。
 *
 * 插件原本挂在设置页里。移到这里的理由：它和"游戏/联机"是同一件事的不同侧面（都要选
 * 游戏、看插件适配了哪些方法），留在设置里既难找，又要和一堆偏好项抢位置。
 */
const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const { queries } = useGameSearch();

const SECTIONS: Record<GameTab, Component> = {
  market: MarketSection,
  games: GamesSection,
  plugins: PluginsSection,
};

/** Tab 图标用与 NavRail 同一套 Material Symbols 名称。 */
const TAB_ICONS: Record<GameTab, string> = {
  market: "storefront",
  games: "sports_esports",
  plugins: "extension",
};

const tab = computed<GameTab>(() =>
  isGameTab(route.params.tab) ? route.params.tab : DEFAULT_GAME_TAB,
);

const tabs = computed(() =>
  GAME_TABS.map((id) => ({ id, icon: TAB_ICONS[id], label: t(`game.tab.${id}`) })),
);

/**
 * Tab 切换方向：决定内容是「向前」（从右滑入）还是「向后」（从左滑入）。
 * 用 Vue 内置 <Transition> 做左右滑动翻页 + 淡入，不写 @keyframes、不手搓 translateX 轨道，
 * 也顺带修掉之前分屏 v-if 延迟挂载导致的「先滑到空白格再 pop」闪屏。
 */
const transitionName = ref<"game-fwd" | "game-back">("game-fwd");
watch(tab, (next, prev) => {
  const ni = GAME_TABS.indexOf(next);
  const pi = prev ? GAME_TABS.indexOf(prev) : -1;
  transitionName.value = ni >= pi ? "game-fwd" : "game-back";
});

// 从侧栏点「游戏」进来时没有 tab 参数：补成默认值，让地址栏与高亮一致。
onMounted(() => {
  if (!isGameTab(route.params.tab)) {
    void router.replace({ name: "game", params: { tab: DEFAULT_GAME_TAB } });
  }
});

function selectTab(id: GameTab) {
  if (id === tab.value) return;
  void router.push({ name: "game", params: { tab: id } });
}

/** Search bar 的清除按钮：清空当前 Tab 自己的搜索词（queries 是按 Tab 各记各的）。 */
function clearSearch() {
  queries[tab.value] = "";
}
</script>

<template>
  <div class="game">
    <div class="game__toolbar">
      <!--
        唯一的搜索框：位置与宽度写死，切 Tab 时它不会移动或改变外观，
        只是把 v-model 换到那个模式自己的搜索词上。
      -->
      <m3e-search-bar clearable class="game__search" @clear="clearSearch">
        <m3e-icon slot="leading" name="search" />
        <input
          slot="input"
          v-model="queries[tab]"
          type="text"
          :placeholder="t(`game.searchPlaceholder.${tab}`)"
          :aria-label="t(`game.searchPlaceholder.${tab}`)"
        />
      </m3e-search-bar>

      <!--
        Tab 条用 @m3e/web 的 <m3e-nav-bar mode="expanded">：只有条目、没有面板，
        所以能像搜索框一样并排放在工具栏里；条目横向（icon + label），选中指示器是
        secondary-container 胶囊，全部由组件按 M3 规范处理。选中态声明式绑定，由路由驱动。
      -->
      <m3e-nav-bar mode="expanded" class="game__tabs">
        <m3e-nav-item
          v-for="item in tabs"
          :key="item.id"
          :selected="item.id === tab"
          @click="selectTab(item.id)"
        >
          <m3e-icon slot="icon" :name="item.icon" />
          {{ item.label }}
        </m3e-nav-item>
      </m3e-nav-bar>
    </div>

    <div class="game__content">
      <!-- 左右滑动翻页 + 淡入：用 Vue 内置 <Transition>，内容按 tab 作 key 重挂载，
           每次切换都会重新播放各分区的逐项进入动画（.stagger）；appear 让首进游戏页也有淡入。 -->
      <Transition :name="transitionName" appear>
        <component :is="SECTIONS[tab]" :key="tab" />
      </Transition>
    </div>
  </div>
</template>

<style scoped>
.game {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.game__toolbar {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: var(--sp-4);
  /* 搜索框用 m3e-search-bar，其容器高度由组件内部决定；通过 --m3e-search-bar-container-height
     压到 40px（与 Tab 按钮同高）。其余内边距只留最小，整条栏尽量矮。 */
  padding: var(--sp-2) var(--sp-4);
}

/* 宽度写死：切换模式时搜索框不得改变外观，也不能因占位文字长短而抖动；
   高度压到和 Tab 按钮一致。 */
.game__search {
  width: 280px;
  flex-shrink: 0;
  --m3e-search-bar-container-height: 40px;
}

/* Tab 条：m3e-nav-bar 默认是"底部导航栏"观感（surfaceContainer 底色、min-height 64px、
   条目居中）。这里压成内联胶囊选择器：容器透明、高度随内容、靠右对齐；选中指示器的配色
   显式接到项目 token，保证与旧手写版观感一致。 */
.game__tabs {
  margin-left: auto;
  --m3e-nav-bar-container-color: transparent;
  --m3e-nav-bar-height: auto;
  --m3e-nav-item-active-container-color: var(--secondary-container);
  --m3e-nav-item-active-label-text-color: var(--on-secondary-container);
  --m3e-nav-item-active-icon-color: var(--on-secondary-container);
}

.game__content {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* Tab 切换：左右滑动翻页 + 淡入（Vue <Transition>，只动 transform/opacity，不写 @keyframes）。
   进入/离场期间两个分区都绝对定位叠放，所以是真正的「整页翻动」，不会出现空白闪一下。 */
.game-fwd-enter-active,
.game-fwd-leave-active,
.game-back-enter-active,
.game-back-leave-active {
  transition: transform var(--motion-medium) var(--ease-standard),
    opacity var(--motion-medium) var(--ease-standard);
  position: absolute;
  inset: 0;
}

.game-fwd-enter-from {
  opacity: 0;
  transform: translateX(100%);
}

.game-fwd-leave-to {
  opacity: 0;
  transform: translateX(-100%);
}

.game-back-enter-from {
  opacity: 0;
  transform: translateX(-100%);
}

.game-back-leave-to {
  opacity: 0;
  transform: translateX(100%);
}
</style>
