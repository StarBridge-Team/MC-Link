<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { listen } from "@tauri-apps/api/event";
import { useConnect } from "../composables/useConnect";
import type { LocalGame } from "../lib/api/types";
import { useIntentStore } from "../stores/intent";
import HostPanel from "./connect/HostPanel.vue";
import MemberPanel from "./connect/MemberPanel.vue";
import StartDialog from "./connect/StartDialog.vue";

/**
 * 联机页外壳：顶部 NavBar 在「房主 / 成员」之间切换（照搬 GameView 的 m3e-nav-bar 写法）。
 *
 * 建房 / 加入成功后**不在这里停留**：命令返回即打开独立房间窗口，主窗口留在本页，
 * 用户可以继续建第二个房间。会话状态不归本页所有（见 `useConnect` 的说明）。
 */
const { t } = useI18n();
const {
  gameQuery,
  inviteValue,
  filteredLocalGames,
  requestedTab,
  requestEntry,
  activeStartProcess,
  clearStartRequest,
  mount,
  applyInvite,
} = useConnect();

/**
 * 房主弹窗的目标游戏。从 `HostPanel` 提到这里，是因为首页的跨页请求也要能打开它
 * （见下方 watch）；弹窗本来就渲染在 `ConnectView` 的模板里，状态放这儿更顺。
 */
const dialogOpen = ref(false);
const activeGame = ref<LocalGame | null>(null);

function openStart(game: LocalGame) {
  activeGame.value = game;
  dialogOpen.value = true;
}

/** Tab 顺序 = NavBar 从左到右的顺序。图标与翻页方向都由它派生。 */
const TAB_ORDER = ["host", "member"] as const;
type ConnectTab = (typeof TAB_ORDER)[number];

const tab = ref<ConnectTab>("host");

/**
 * 托盘面板的「最近游戏」：后端发来一条 `connect` 意图，这里落到房主页并预选进程。
 *
 * 用 `watch` 而不是只在挂载时取一次：用户可能本来就停在联机页，
 * 那时组件不会重新挂载，只有 watch 能收到。
 */
const intent = useIntentStore();
watch(
  () => intent.pending,
  (value) => {
    if (!value || value.action !== "connect") return;
    intent.take();
    tab.value = "host";
    requestEntry("host", value.process ?? undefined);
  },
  { immediate: true },
);

/**
 * 面板切换方向：按 Tab 在 NavBar 上的左右先后决定往哪边翻。
 *
 * 判断放在 `watch` 里而不是点击处理里——深链（`mclink://join/...`）切 Tab 走的不是
 * 点击路径，只有 watch 能把所有切换来源都覆盖到。
 */
const transitionName = ref<"connect-fwd" | "connect-back">("connect-fwd");
watch(tab, (next, prev) => {
  const pi = prev ? TAB_ORDER.indexOf(prev) : -1;
  transitionName.value = TAB_ORDER.indexOf(next) >= pi ? "connect-fwd" : "connect-back";
});

/**
 * 一个搜索框，按当前 Tab 切换含义（照 GameView 的做法：位置固定、只换绑定）：
 * 房主模式筛本机游戏进程，成员模式录入邀请码。
 */
const searchValue = computed({
  get: () => (tab.value === "host" ? gameQuery.value : inviteValue.value),
  set: (value: string) => {
    if (tab.value === "host") gameQuery.value = value;
    else inviteValue.value = value;
  },
});

/**
 * 提示文案。成员侧**刻意不用适配器给的 `placeholder`**（如 `ABCD-1234`）：
 * 各适配器的邀请码格式不同，预先摆一个样例会让人以为格式是固定的。
 */
const searchPlaceholder = computed(() =>
  tab.value === "host" ? t("connect.searchGames") : t("connect.searchInvite"),
);

/**
 * Tab 图标（Material Symbols 名）。`m3e-nav-item` 的图标槽是可选的，
 * 不传就只剩文字——照 GameView 的做法补上，保持两页 Tab 条一致。
 */
const TAB_ICONS: Record<ConnectTab, string> = {
  host: "wifi_tethering",
  member: "group",
};

const tabs = computed(() =>
  TAB_ORDER.map((id) => ({ id, icon: TAB_ICONS[id], label: t(`connect.tab.${id}`) })),
);

/**
 * 首页点「开始联机」后落到的 Tab。
 * 在 `onMounted` 里同步消费——放在 `await mount()` 之后的话，用户会先看到房主页闪一下。
 */
if (requestedTab.value) {
  tab.value = requestedTab.value;
  requestedTab.value = null;
}

/**
 * 若首页指定了要联机的进程，等扫描结果里出现它，就替用户按下那张卡片的「开始联机」。
 *
 * 用 `watch` 而不是"挂载后查一次"：扫描是异步的，匹配往往在挂载之后才出现。
 * 超时由 `activeStartProcess()` 的 TTL 兜底（游戏已退出时不会永远等待）。
 */
watch(filteredLocalGames, (games) => {
  const target = activeStartProcess();
  if (!target) return;
  const hit = games.find((g) => g.process === target);
  if (!hit) return;
  clearStartRequest();
  openStart(hit);
});

let unlistenDeep: (() => void) | null = null;

onMounted(async () => {
  await mount();
  unlistenDeep = await listen<{ action: string; params: string[] }>("deep-link", (e) => {
    if (e.payload.action === "join" && e.payload.params[0]) {
      tab.value = "member";
      applyInvite(e.payload.params[0]);
    }
  });
});

onUnmounted(() => {
  unlistenDeep?.();
});
</script>

<template>
  <div class="connect">
    <div class="connect__toolbar">
      <m3e-search-bar
        clearable
        class="connect__search"
        @clear="searchValue = ''"
      >
        <m3e-icon slot="leading" name="search" />
        <input
          slot="input"
          v-model="searchValue"
          type="text"
          :placeholder="searchPlaceholder"
          :aria-label="searchPlaceholder"
        />
      </m3e-search-bar>

      <m3e-nav-bar mode="expanded" class="connect__tabs">
        <m3e-nav-item
          v-for="item in tabs"
          :key="item.id"
          :selected="item.id === tab"
          @click="tab = item.id"
        >
          <m3e-icon slot="icon" :name="item.icon" />
          {{ item.label }}
        </m3e-nav-item>
      </m3e-nav-bar>
    </div>

    <div class="connect__content">
      <!-- 房主 / 成员 两个面板。会话本身在独立窗口里，本页不再有"已连接"态。 -->
      <Transition :name="transitionName" appear mode="out-in">
        <HostPanel v-if="tab === 'host'" key="host" @start="openStart" />
        <MemberPanel v-else key="member" />
      </Transition>

      <!-- 房主建房间的弹窗（由 ConnectView 持有状态，见上方 openStart） -->
      <StartDialog
        v-if="dialogOpen"
        mode="host"
        :game="activeGame"
        :open="dialogOpen"
        @close="dialogOpen = false"
      />
    </div>
  </div>
</template>

<style scoped>
.connect {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.connect__toolbar {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: var(--sp-4);
  padding: var(--sp-2) var(--sp-4);
}

/* 宽度写死：切 Tab 时搜索框不得改变外观，也不能因提示文字长短而抖动；
   高度压到与 Tab 按钮一致。 */
.connect__search {
  width: 280px;
  flex-shrink: 0;
  --m3e-search-bar-container-height: 40px;
}

.connect__tabs {
  margin-left: auto;
  --m3e-nav-bar-container-color: transparent;
  --m3e-nav-bar-height: auto;
  --m3e-nav-item-active-container-color: var(--secondary-container);
  --m3e-nav-item-active-label-text-color: var(--on-secondary-container);
  --m3e-nav-item-active-icon-color: var(--on-secondary-container);
}

.connect__content {
  position: relative;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

/* 面板切换：左右滑动翻页 + 淡入（仅动 transform/opacity，不写 @keyframes）。
   方向跟 NavBar 的左右顺序走：往后一个 Tab 向右翻，往前一个向左翻。
   `mode="out-in"` 让两块不同时存在，所以不需要绝对定位叠放。 */
.connect-fwd-enter-active,
.connect-fwd-leave-active,
.connect-back-enter-active,
.connect-back-leave-active {
  transition: transform var(--motion-medium) var(--ease-standard),
    opacity var(--motion-medium) var(--ease-standard);
}
.connect-fwd-enter-from {
  opacity: 0;
  transform: translateX(40px);
}
.connect-fwd-leave-to {
  opacity: 0;
  transform: translateX(-40px);
}
.connect-back-enter-from {
  opacity: 0;
  transform: translateX(-40px);
}
.connect-back-leave-to {
  opacity: 0;
  transform: translateX(40px);
}
</style>
