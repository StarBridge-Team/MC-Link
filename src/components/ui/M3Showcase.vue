<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { ElMessage, ElMessageBox, ElNotification } from "element-plus";
import {
  Home,
  Wifi,
  Settings,
  Palette,
  Plus,
  Heart,
  Search,
  Star,
  Share2,
  Check,
  Bell,
  User,
  Menu as MenuIcon,
} from "lucide-vue-next";
import { useM3Theme, M3_VARIANTS, M3_PRESET_SEEDS } from "../../composables/useM3Theme";
import type { M3Roles } from "../../lib/m3/types";

const {
  seed,
  variant,
  contrast,
  isDark,
  scheme,
  loading,
  source,
  regenerate,
  setSeed,
  setVariant,
  setContrast,
  toggleDark,
  exportConfig,
} = useM3Theme();

const customSeed = ref(seed.value);
const dialogVisible = ref(false);
const drawerVisible = ref(false);
const activeTab = ref("one");

const tableData = [
  { name: "生存服", mode: "创造", latency: "12ms" },
  { name: "空岛服", mode: "生存", latency: "38ms" },
  { name: "红石服", mode: "冒险", latency: "24ms" },
];

onMounted(async () => {
  await regenerate();
});

function applyCustomSeed() {
  if (/^#?[0-9a-fA-F]{3}([0-9a-fA-F]{3})?$/.test(customSeed.value.trim())) {
    setSeed(customSeed.value.trim().replace(/^#/, "#"));
  } else {
    ElMessage.warning("请输入合法的十六进制颜色，如 #6750A4");
  }
}

const toneOrder = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 95, 99, 100];
const paletteKeys = computed(() =>
  scheme.value
    ? (Object.keys(scheme.value.palettes) as Array<
        keyof typeof scheme.value.palettes
      >)
    : []
);

const rolePreview: Array<{ label: string; key: keyof M3Roles }> = [
  { label: "Primary", key: "primary" },
  { label: "On Primary", key: "on_primary" },
  { label: "Primary Container", key: "primary_container" },
  { label: "On Primary Container", key: "on_primary_container" },
  { label: "Secondary", key: "secondary" },
  { label: "Tertiary", key: "tertiary" },
  { label: "Error", key: "error" },
  { label: "Surface", key: "surface" },
  { label: "Surface Variant", key: "surface_variant" },
  { label: "Outline", key: "outline" },
  { label: "Inverse Surface", key: "inverse_surface" },
];

const roleValue = (k: keyof M3Roles) =>
  scheme.value ? scheme.value[isDark.value ? "dark" : "light"][k] : "#000";

const exportedJson = computed(() => exportConfig());

function copyConfig() {
  navigator.clipboard?.writeText(exportedJson.value).then(
    () => ElMessage.success("已复制 M3 配置 JSON"),
    () => ElMessage.error("复制失败")
  );
}

function showDialog() {
  dialogVisible.value = true;
}
function confirmAction() {
  ElMessageBox.confirm("这是一个 M3 风格的确认对话框示例。", "确认操作", {
    confirmButtonText: "确定",
    cancelButtonText: "取消",
    type: "warning",
  })
    .then(() => ElMessage.success("已确认"))
    .catch(() => {});
}
function ElNotificationDemo() {
  ElNotification.success({ title: "通知", message: "这是一条 M3 通知" });
}
</script>

<template>
  <div class="m3-showcase" :class="{ 'm3-theme': true, dark: isDark }">
    <!-- 顶部应用栏 -->
    <header class="m3-top-app-bar showcase-bar">
      <el-icon :size="24"><Palette /></el-icon>
      <div class="bar-title">Material Design 3 设计系统</div>
      <div class="bar-spacer" />
      <el-switch
        :model-value="isDark"
        @change="toggleDark"
        inline-prompt
        active-text="暗"
        inactive-text="亮"
      />
      <span class="source-badge" :class="source">{{ source === 'backend' ? 'Rust 后端' : '前端引擎' }}</span>
    </header>

    <div class="showcase-body">
      <!-- 控制面板 -->
      <aside class="control-panel m3-surface-container">
        <h3 class="section-title">配色生成</h3>
        <p class="m3-text-secondary hint-text">后端使用 material-colors crate 从种子色生成完整 M3 方案，前端自动同步。</p>

        <div class="field-group" style="margin-top: 12px">
          <label class="field-group__label">种子颜色（Seed）</label>
          <div class="seed-presets">
            <button
              v-for="p in M3_PRESET_SEEDS"
              :key="p"
              class="seed-dot"
              :class="{ active: seed.toLowerCase() === p.toLowerCase() }"
              :style="{ background: p }"
              @click="setSeed(p)"
            />
          </div>
          <div class="custom-seed">
            <input type="color" v-model="customSeed" class="color-input" />
            <el-input v-model="customSeed" size="small" placeholder="#6750A4" @keyup.enter="applyCustomSeed" />
            <el-button size="small" @click="applyCustomSeed">应用</el-button>
          </div>
        </div>

        <div class="field-group">
          <label class="field-group__label">变体（Variant）</label>
          <el-select :model-value="variant" @change="(v: any) => setVariant(v)" size="default">
            <el-option
              v-for="v in M3_VARIANTS"
              :key="v.value"
              :label="v.label"
              :value="v.value"
            />
          </el-select>
        </div>

        <div class="field-group">
          <label class="field-group__label">
            对比度 ({{ contrast.toFixed(2) }})
          </label>
          <el-slider :model-value="contrast" :min="-1" :max="1" :step="0.1" @input="setContrast" />
        </div>

        <el-button type="primary" :loading="loading" @click="regenerate" style="width: 100%; margin-top: 8px">
          重新生成方案
        </el-button>

        <div class="field-group" style="margin-top: 16px">
          <label class="field-group__label">导出配置（前后端同步）</label>
          <el-input
            :model-value="exportedJson"
            type="textarea"
            :rows="6"
            readonly
            class="json-box"
          />
          <el-button size="small" @click="copyConfig" style="margin-top: 6px">复制 JSON</el-button>
        </div>
      </aside>

      <!-- 主展示区 -->
      <main class="showcase-main">
        <!-- 按钮 -->
        <section class="m3-card demo-section">
          <h3 class="section-title">按钮 Buttons</h3>
          <div class="btn-row">
            <el-button type="primary">Filled</el-button>
            <el-button>Default（Tonal）</el-button>
            <el-button type="primary" plain>Outlined</el-button>
            <el-button type="primary" text>Text</el-button>
            <el-button type="success">Success</el-button>
            <el-button type="warning">Warning</el-button>
            <el-button type="danger">Danger</el-button>
            <el-button :icon="Plus" circle />
            <el-button type="primary" :icon="Heart">喜欢</el-button>
          </div>
          <div class="btn-row">
            <el-button size="small">Small</el-button>
            <el-button>Default</el-button>
            <el-button size="large">Large</el-button>
            <el-button disabled>Disabled</el-button>
            <el-button loading>Loading</el-button>
          </div>
        </section>

        <!-- 卡片 -->
        <section class="m3-card demo-section">
          <h3 class="section-title">卡片 Cards</h3>
          <div class="card-row">
            <el-card shadow="hover" header="联机房间">
              <p class="m3-text-secondary">成员已连接，延迟 12ms。</p>
              <template #footer>
                <el-button type="primary" text>管理</el-button>
              </template>
            </el-card>
            <el-card shadow="hover" class="stat-card">
              <div class="stat-num">{{ scheme ? 'M3' : '--' }}</div>
              <div class="m3-text-secondary">Material 3</div>
            </el-card>
          </div>
        </section>

        <!-- 表单 / 输入 -->
        <section class="m3-card demo-section">
          <h3 class="section-title">输入框与表单 Inputs</h3>
          <el-row :gutter="16">
            <el-col :xs="24" :sm="12">
              <el-input placeholder="搜索服务器…" :prefix-icon="Search" />
            </el-col>
            <el-col :xs="24" :sm="12">
              <el-input type="password" show-password v-model="customSeed" placeholder="密码" />
            </el-col>
            <el-col :xs="24" :sm="12" style="margin-top: 12px">
              <el-select placeholder="选择区域" style="width: 100%">
                <el-option label="东亚" value="ea" />
                <el-option label="欧洲" value="eu" />
                <el-option label="北美" value="na" />
              </el-select>
            </el-col>
            <el-col :xs="24" :sm="12" style="margin-top: 12px">
              <el-input type="textarea" :rows="2" placeholder="备注信息" />
            </el-col>
          </el-row>
          <div class="inline-controls">
            <el-switch v-model="drawerVisible" active-text="开关" />
            <el-checkbox v-model="dialogVisible">复选框</el-checkbox>
            <el-radio-group v-model="customSeed">
              <el-radio value="#6750A4">A</el-radio>
              <el-radio value="#0061A4">B</el-radio>
            </el-radio-group>
            <el-segmented :options="['日', '周', '月']" />
          </div>
        </section>

        <!-- 导航 -->
        <section class="m3-card demo-section">
          <h3 class="section-title">导航 Navigation</h3>
          <div class="nav-demo">
            <nav class="m3-nav-rail">
              <div class="m3-nav-item is-active"><el-icon :size="22"><Home /></el-icon>首页</div>
              <div class="m3-nav-item"><el-icon :size="22"><Wifi /></el-icon>联机</div>
              <div class="m3-nav-item"><el-icon :size="22"><Bell /></el-icon>通知</div>
              <div class="m3-nav-item"><el-icon :size="22"><User /></el-icon>我的</div>
            </nav>
            <el-menu class="nav-menu" default-active="1">
              <el-menu-item index="1"><el-icon><Home /></el-icon><span>概览</span></el-menu-item>
              <el-menu-item index="2"><el-icon><Settings /></el-icon><span>设置</span></el-menu-item>
              <el-menu-item index="3"><el-icon><Star /></el-icon><span>收藏</span></el-menu-item>
            </el-menu>
            <div class="nav-chips">
              <span class="m3-chip is-active"><el-icon><Check /></el-icon>已选</span>
              <span class="m3-chip m3-chip--assist"><el-icon><Share2 /></el-icon>分享</span>
              <span class="m3-chip"><el-icon><MenuIcon /></el-icon>菜单</span>
              <button class="m3-fab"><el-icon :size="24"><Plus /></el-icon></button>
            </div>
          </div>
        </section>

        <!-- 反馈 -->
        <section class="m3-card demo-section">
          <h3 class="section-title">反馈 Feedback</h3>
          <div class="btn-row">
            <el-button @click="showDialog">打开对话框</el-button>
            <el-button @click="confirmAction">确认框</el-button>
            <el-button @click="ElMessage.success('操作成功')">Message</el-button>
            <el-button @click="ElMessage.warning('请注意')">Warning</el-button>
            <el-button @click="ElMessage.error('出错了')">Error</el-button>
            <el-button @click="ElNotificationDemo">Notification</el-button>
          </div>
          <el-progress :percentage="68" style="margin-top: 12px; max-width: 360px" />
          <el-alert
            style="margin-top: 12px"
            title="M3 风格提示条"
            type="success"
            :closable="false"
            show-icon
          />
        </section>

        <!-- 扩展组件 -->
        <section class="m3-card demo-section">
          <h3 class="section-title">扩展组件 More Components</h3>
          <el-tabs v-model="activeTab">
            <el-tab-pane label="表格" name="one">
              <el-table :data="tableData" size="default" style="width: 100%">
                <el-table-column prop="name" label="服务器" />
                <el-table-column prop="mode" label="模式" width="120" />
                <el-table-column prop="latency" label="延迟" width="100" />
              </el-table>
              <el-pagination
                layout="prev, pager, next"
                :total="42"
                :page-size="10"
                style="margin-top: 12px"
              />
            </el-tab-pane>
            <el-tab-pane label="步骤" name="two">
              <el-steps :active="1" finish-status="success" simple>
                <el-step title="连接" />
                <el-step title="配置" />
                <el-step title="启动" />
              </el-steps>
              <el-timeline style="margin-top: 16px">
                <el-timeline-item timestamp="刚刚" placement="top">
                  <el-card>已同步 M3 配色方案</el-card>
                </el-timeline-item>
                <el-timeline-item timestamp="1 分钟前" placement="top">
                  <el-card>应用主题到 Element Plus</el-card>
                </el-timeline-item>
              </el-timeline>
            </el-tab-pane>
            <el-tab-pane label="其他" name="three">
              <el-collapse>
                <el-collapse-item title="M3 表面色如何映射？" name="1">
                  <div class="m3-text-secondary">
                    surface / surface-container-* 依次映射到 Element Plus 的背景与浮层变量。
                  </div>
                </el-collapse-item>
                <el-collapse-item title="明暗主题如何切换？" name="2">
                  <div class="m3-text-secondary">
                    通过 .dark class 切换，CSS 变量自动从 -l 切到 -d 后缀。
                  </div>
                </el-collapse-item>
              </el-collapse>
              <div class="btn-row" style="margin-top: 16px">
                <el-dropdown>
                  <el-button>下拉菜单 <el-icon><MenuIcon /></el-icon></el-button>
                  <template #dropdown>
                    <el-dropdown-menu>
                      <el-dropdown-item>重命名</el-dropdown-item>
                      <el-dropdown-item>复制</el-dropdown-item>
                      <el-dropdown-item divided>删除</el-dropdown-item>
                    </el-dropdown-menu>
                  </template>
                </el-dropdown>
                <el-badge :value="12" class="badge-demo">
                  <el-button :icon="Bell" circle />
                </el-badge>
                <el-avatar :size="36">M3</el-avatar>
              </div>
              <el-breadcrumb separator="/" style="margin-top: 14px">
                <el-breadcrumb-item>首页</el-breadcrumb-item>
                <el-breadcrumb-item>设置</el-breadcrumb-item>
                <el-breadcrumb-item>外观</el-breadcrumb-item>
              </el-breadcrumb>
              <div class="skeleton-demo">
                <el-skeleton :rows="2" animated />
              </div>
            </el-tab-pane>
          </el-tabs>
        </section>

        <!-- 调色板 -->
        <section class="m3-card demo-section">
          <h3 class="section-title">色调调色板 Tonal Palette</h3>
          <p class="m3-text-secondary hint-text">由种子色生成的完整 M3 调色板（0–100 色阶）。</p>
          <div class="palette-grid">
            <div v-for="key in paletteKeys" :key="key" class="palette-col">
              <div class="palette-name">{{ key }}</div>
              <div
                v-for="t in toneOrder"
                :key="t"
                class="swatch"
                :style="{ background: scheme?.palettes[key].tones[t] }"
                :title="`T${t}: ${scheme?.palettes[key].tones[t]}`"
              >
                <span v-if="t === 40 || t === 80" class="swatch-label">{{ t }}</span>
              </div>
            </div>
          </div>
        </section>

        <!-- 角色对照 -->
        <section class="m3-card demo-section">
          <h3 class="section-title">颜色角色 Roles（{{ isDark ? 'Dark' : 'Light' }}）</h3>
          <div class="role-grid">
            <div v-for="r in rolePreview" :key="r.key" class="role-item">
              <div class="role-swatch" :style="{ background: roleValue(r.key) }" />
              <div class="role-meta">
                <div class="role-key">{{ r.label }}</div>
                <div class="role-val">{{ roleValue(r.key) }}</div>
              </div>
            </div>
          </div>
        </section>
      </main>
    </div>

    <el-dialog v-model="dialogVisible" title="M3 对话框" width="420px">
      <p>这是一个符合 Material Design 3 风格的对话框示例。</p>
      <template #footer>
        <el-button @click="dialogVisible = false">取消</el-button>
        <el-button type="primary" @click="dialogVisible = false">确定</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<style scoped>
.m3-showcase {
  height: 100%;
  display: flex;
  flex-direction: column;
  background-color: var(--m3-background, #fef7ff);
  color: var(--m3-on-background, #1c1b1f);
  overflow: hidden;
}
.showcase-bar {
  border-radius: 0;
  position: sticky;
  top: 0;
  z-index: 5;
}
.bar-title {
  font-size: 18px;
  font-weight: 600;
  letter-spacing: 0.2px;
}
.bar-spacer {
  flex: 1;
}
.source-badge {
  font-size: 12px;
  padding: 2px 10px;
  border-radius: 9999px;
  background: var(--m3-secondary-container);
  color: var(--m3-on-secondary-container);
}
.source-badge.backend {
  background: var(--m3-primary-container);
  color: var(--m3-on-primary-container);
}
.showcase-body {
  flex: 1;
  display: flex;
  gap: 16px;
  padding: 16px;
  overflow: hidden;
  min-height: 0;
}
.control-panel {
  width: 320px;
  flex-shrink: 0;
  border-radius: var(--m3-shape-corner-large);
  padding: 16px;
  overflow-y: auto;
}
.showcase-main {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
  padding-right: 4px;
}
.demo-section {
  padding: 20px;
}
.hint-text {
  font-size: 13px;
  margin: 0 0 4px;
}
.seed-presets {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin: 8px 0;
}
.seed-dot {
  width: 28px;
  height: 28px;
  border-radius: 9999px;
  border: 2px solid transparent;
  cursor: pointer;
  transition: transform 0.15s;
}
.seed-dot.active {
  border-color: var(--m3-on-surface);
  transform: scale(1.1);
}
.custom-seed {
  display: flex;
  gap: 6px;
  align-items: center;
}
.color-input {
  width: 36px;
  height: 32px;
  border: 1px solid var(--m3-outline-variant);
  border-radius: 8px;
  background: none;
  padding: 2px;
  cursor: pointer;
}
.json-box :deep(.el-textarea__inner) {
  font-family: monospace;
  font-size: 11px;
}
.btn-row {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
  margin-bottom: 12px;
}
.card-row {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
}
.stat-card {
  width: 160px;
  text-align: center;
}
.stat-num {
  font-size: 32px;
  font-weight: 700;
  color: var(--m3-primary);
}
.inline-controls {
  display: flex;
  flex-wrap: wrap;
  gap: 20px;
  align-items: center;
  margin-top: 16px;
}
.nav-demo {
  display: flex;
  gap: 20px;
  align-items: flex-start;
  flex-wrap: wrap;
}
.nav-menu {
  width: 220px;
  border-radius: var(--m3-shape-corner-large);
  background: var(--m3-surface-container);
}
.nav-chips {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.palette-grid {
  display: flex;
  gap: 8px;
  overflow-x: auto;
  padding-bottom: 8px;
}
.palette-col {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex-shrink: 0;
}
.palette-name {
  font-size: 10px;
  color: var(--m3-on-surface-variant);
  text-align: center;
  margin-bottom: 4px;
  width: 44px;
}
.swatch {
  width: 44px;
  height: 22px;
  border-radius: 3px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.swatch-label {
  font-size: 9px;
  color: rgba(0, 0, 0, 0.55);
  mix-blend-mode: difference;
}
.role-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 10px;
}
.role-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 8px;
  border-radius: 10px;
  background: var(--m3-surface-container-low);
}
.role-swatch {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  flex-shrink: 0;
  border: 1px solid var(--m3-outline-variant);
}
.role-key {
  font-size: 12px;
  font-weight: 600;
}
.role-val {
  font-size: 11px;
  color: var(--m3-on-surface-variant);
  font-family: monospace;
}
.badge-demo {
  margin-top: 4px;
}
.skeleton-demo {
  margin-top: 16px;
  padding: 12px;
  border-radius: var(--m3-shape-corner-medium);
  background: var(--m3-surface-container-low);
}

@media (max-width: 860px) {
  .showcase-body {
    flex-direction: column;
    overflow-y: auto;
  }
  .control-panel {
    width: 100%;
  }
}
</style>
