<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";

import WidgetNodeView from "./WidgetNodeView.vue";
import { mountWidget, sendWidgetAction, unmountWidget } from "../../lib/api/pluginWidget";
import type { WidgetView } from "../../lib/api/types";

/**
 * 第三方挂件宿主。
 *
 * # 契约
 *
 * 1. 挂载时调 `widget_mount`，插件返回**声明式节点树**，这里只渲染白名单类型；
 * 2. 用户点按钮 → `widget_action` 回传动作名，插件可返回新视图（`null` 表示不刷新）；
 * 3. 卸载时调 `widget_unmount`（失败不影响卸载）。
 *
 * # 为什么不用 v-html / 动态组件
 *
 * 插件交的东西一律当作**数据**。渲染层只要有一次"顺手支持一下 HTML"，整套
 * 权限模型就没了——挂件跑在主窗口里，能直接读 DOM 与会话。
 *
 * # 状态三态
 *
 * 加载中 / 失败（带重试）/ 空视图，必须分开渲染：把"插件没起来"显示成"挂件为空"
 * 会让用户以为插件没内容。
 */
const props = withDefaults(
  defineProps<{
    pluginId: string;
    widgetId: string;
    /** 是否在标题区显示插件 ID（调试用）。 */
    showSource?: boolean;
  }>(),
  { showSource: false },
);

const emit = defineEmits<{ (event: "error", message: string): void }>();

const view = ref<WidgetView | null>(null);
const loading = ref(true);
const error = ref<string | null>(null);
const busy = ref(false);

async function load() {
  loading.value = true;
  error.value = null;
  try {
    view.value = await mountWidget(props.pluginId, props.widgetId);
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
    emit("error", error.value);
  } finally {
    loading.value = false;
  }
}

/** 按钮点击：上行动作，插件的返回值决定是否刷新视图。 */
async function onAction(action: string) {
  if (busy.value) return;
  busy.value = true;
  try {
    const next = await sendWidgetAction(props.pluginId, props.widgetId, action);
    if (next) view.value = next;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
    emit("error", error.value);
  } finally {
    busy.value = false;
  }
}

async function unload() {
  try {
    await unmountWidget(props.pluginId, props.widgetId);
  } catch {
    // 卸载是宿主单方面的决定，插件侧失败不阻断
  }
}

watch(
  () => [props.pluginId, props.widgetId],
  () => {
    void unload();
    void load();
  },
);

onBeforeUnmount(() => {
  void unload();
});

defineExpose({ refresh: load });

void load();
</script>

<template>
  <section class="host" :aria-busy="loading || busy">
    <header v-if="view?.title || props.showSource" class="host__head">
      <div class="host__titles">
        <h3 v-if="view?.title" class="host__title">{{ view.title }}</h3>
        <p v-if="view?.subtitle" class="host__subtitle">{{ view.subtitle }}</p>
      </div>
      <span v-if="props.showSource" class="host__source mono">{{ props.pluginId }}</span>
    </header>

    <div v-if="loading" class="host__state">正在加载挂件…</div>

    <div v-else-if="error" class="host__state host__state--error">
      <span>{{ error }}</span>
      <m3e-button size="small" @click="load">重试</m3e-button>
    </div>

    <div v-else-if="view && view.nodes.length > 0" class="host__body">
      <WidgetNodeView
        v-for="(node, index) in view.nodes"
        :key="node.id ?? index"
        :node="node"
        :on-action="onAction"
      />
    </div>

    <div v-else class="host__state">该插件暂无可显示的内容</div>
  </section>
</template>

<style scoped>
.host {
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
  padding: var(--sp-4);
  border-radius: var(--shape-corner-m);
  background: var(--card-bg);
}

.host__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--sp-3);
}

.host__titles {
  display: flex;
  flex-direction: column;
  gap: var(--sp-1);
}

.host__title {
  margin: 0;
  font-size: var(--fs-label);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
}

.host__subtitle {
  margin: 0;
  font-size: var(--fs-caption);
  color: var(--text-muted);
}

.host__source {
  font-size: var(--fs-caption);
  color: var(--text-muted);
}

.host__body {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}

.host__state {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  font-size: var(--fs-base);
  color: var(--text-muted);
}

.host__state--error {
  color: var(--error);
  flex-wrap: wrap;
}
</style>
