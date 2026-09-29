# Element Plus 组件库 — AI 编程参考手册

> \*\*版本\*\*: 2.14.1 | \*\*框架\*\*: Vue 3 | \*\*官网\*\*: https://cn.element-plus.org/zh-CN/
>
> 本文档面向 AI 助手，提供 Element Plus 全部 \*\*82 个组件\*\* 的速查指南。每个组件包含：用途说明、核心 API 概要、典型代码示例、常见使用场景。

\---

## 目录

* [一、Basic 基础组件（12个）](#一basic-基础组件12个)
* [二、Config 配置组件（1个）](#二config-配置组件1个)
* [三、Form 表单组件（25个）](#三form-表单组件25个)
* [四、Data 数据展示（23个）](#四data-数据展示23个)
* [五、Navigation 导航（9个）](#五navigation-导航9个)
* [六、Feedback 反馈组件（10个）](#六feedback-反馈组件10个)
* [七、Others 其他（2个）](#七others-其他2个)
* [附录：通用约定与最佳实践](#附录通用约定与最佳实践)

\---

## 一、Basic 基础组件（12个）

### 1\. Button 按钮

**用途**: 触发操作的基础交互元素，支持多种类型、尺寸、状态。

```vue
<template>
  <!-- 基础用法 -->
  <el-button>默认按钮</el-button>
  <el-button type="primary">主要按钮</el-button>
  <el-button type="success">成功</el-button>
  <el-button type="warning">警告</el-button>
  <el-button type="danger">危险</el-button>
  <el-button type="info">信息</el-button>

  <!-- 文字按钮 -->
  <el-button type="primary" link>文字链接</el-button>

  <!-- 尺寸 -->
  <el-button size="large">大</el-button>
  <el-button size="default">默认</el-button>
  <el-button size="small">小</el-button>

  <!-- 禁用/加载 -->
  <el-button disabled>禁用</el-button>
  <el-button :loading="true">加载中</el-button>

  <!-- 图标按钮 -->
  <el-button :icon="Search" circle />
  <el-button type="primary" :icon="Edit" />

  <!-- 按钮组 -->
  <el-button-group>
    <el-button>左</el-button>
    <el-button>中</el-button>
    <el-button>右</el-button>
  </el-button-group>
</template>

<script setup>
import { Search, Edit } from '@element-plus/icons-vue'
</script>
```

|Prop|类型|默认值|说明|
|-|-|-|-|
|size|`large / default / small`|-|尺寸|
|type|`primary / success / warning / danger / info`|-|类型|
|plain|boolean|false|朴素按钮|
|round|boolean|false|圆角|
|link|boolean|false|链接按钮|
|loading|boolean|false|加载中|
|disabled|boolean|false|禁用|
|icon|Component|-|图标组件|

\---

### 2\. Border 边框

**用途**: 提供带边框的容器样式，用于视觉分隔或强调区域。

```vue
<template>
  <div class="border">带边框的内容</div>
</template>

<style scoped>
.border {
  border: 1px solid var(--el-border-color);
  border-radius: var(--el-border-radius-base);
  padding: 20px;
}
</style>
```

\---

### 3\. Color 色彩

**用途**: 定义品牌色彩规范和调色板，用于统一 UI 色彩体系。

```vue
<script setup>
// Element Plus 内置 CSS 变量：
// --el-color-primary: #409EFF (主色)
// --el-color-success: #67C23A
// --el-color-warning: #E6A23C
// --el-color-danger: #F56C6C
// --el-color-info: #909399
</script>

<style>
/\* 使用内置色彩 \*/
.my-primary { color: var(--el-color-primary); }
.my-bg { background-color: var(--el-color-primary-light-9); }
</style>
```

\---

### 4\. Container 布局容器

**用途**: 快速搭建页面基本结构（Header / Aside / Main / Footer）。

```vue
<template>
  <el-container>
    <el-header>Header</el-header>
    <el-container>
      <el-aside width="200px">Aside</el-aside>
      <el-main>Main</el-main>
    </el-container>
    <el-footer>Footer</el-footer>
  </el-container>
</template>
```

|组件|说明|
|-|-|
|`<el-container>`|外层容器|
|`<el-header>`|顶栏|
|`<el-aside>`|侧边栏|
|`<el-main>`|主内容区|
|`<el-footer>`|底栏|

\---

### 5\. Icon 图标

**用途**: 提供丰富的 SVG 图标集合。

```vue
<template>
  <!-- 直接使用图标组件 -->
  <el-icon :size="20"><Edit /></el-icon>
  <el-icon color="#409EFF"><Search /></el-icon>

  <!-- 通过 icon 属性使用 -->
  <el-button :icon="Edit">编辑</el-button>

  <!-- 图标集合 -->
  <el-icon><Plus /></el-icon>
  <el-icon><Minus /></el-icon>
  <el-icon><Close /></el-icon>
  <el-icon><Check /></el-icon>
  <el-icon><ArrowLeft /></el-icon>
  <el-icon><ArrowRight /></el-icon>
  <el-icon><ArrowUp /></el-icon>
  <el-icon><ArrowDown /></el-icon>
  <el-icon><Refresh /></el-icon>
  <el-icon><Delete /></el-icon>
  <el-icon><Setting /></el-icon>
  <el-icon><User /></el-icon>
  <el-icon><Phone /></el-icon>
  <el-icon><Message /></el-icon>
  <el-icon><Location /></el-icon>
  <el-icon><Star /></el-icon>
  <el-icon><Lock /></el-icon>
  <el-icon><Unlock /></el-icon>
  <el-icon><View /></el-icon>
  <el-icon><Hide /></el-icon>
</template>

<script setup>
import {
  Edit, Search, Plus, Minus, Close, Check,
  ArrowLeft, ArrowRight, ArrowUp, ArrowDown,
  Refresh, Delete, Setting, User, Phone,
  Message, Location, Star, Lock, Unlock,
  View, Hide
} from '@element-plus/icons-vue'
</script>
```

> \*\*安装\*\*: `npm install @element-plus/icons-vue`
>
> \*\*全局注册\*\*: `import \* as ElementPlusIconsVue from '@element-plus/icons-vue'` 然后遍历 `app.component()`

\---

### 6\. Layout 栅格布局

**用途**: 基于 24 栅格系统的响应式布局。

```vue
<template>
  <el-row :gutter="20">
    <el-col :span="12">
      <div class="grid-content">占 12 格</div>
    </el-col>
    <el-col :span="12">
      <div class="grid-content">占 12 格</div>
    </el-col>
  </el-row>

  <!-- 响应式 -->
  <el-row :gutter="10">
    <el-col :xs="24" :sm="12" :md="8" :lg="6">
      <div>响应式列</div>
    </el-col>
  </el-row>

  <!-- 对齐方式 -->
  <el-row justify="center" align="middle">
    <el-col :span="4"><div>居中内容</div></el-col>
  </el-row>
</template>
```

|Row Props|说明|
|-|-|
|gutter|栅格间距（px）|
|justify|`start / center / end / space-around / space-between / space-evenly`|
|align|`top / middle / bottom`|

|Col Props|说明|
|-|-|
|span|占据格数（1-24）|
|offset|偏移格数|
|push / pull|推/拉格数|
|xs / sm / md / lg / xl / xxl|响应式断点|

\---

### 7\. Link 链接

**用途**: 文字超链接，增强版 `<a>` 标签。

```vue
<template>
  <el-link href="https://example.com" target="\_blank">默认链接</el-link>
  <el-link type="primary">主要链接</el-link>
  <el-link type="success">成功链接</el-link>
  <el-link type="warning">警告链接</el-link>
  <el-link type="danger">危险链接</el-link>
  <el-link type="info">信息链接</el-link>
  <el-link :underline="false">无下划线</el-link>
  <el-link disabled>禁用链接</el-link>
  <el-link :icon="Edit">带图标</el-link>
</template>
```

\---

### 8\. Text 文本（v2.3.0+）

**用途**: 用于文本展示，支持多种样式、截断、高亮等。

```vue
<template>
  <el-text>普通文本</el-text>
  <el-text type="primary">主要文本</el-text>
  <el-text type="success">成功文本</el-text>
  <el-text type="warning">警告文本</el-text>
  <el-text type="danger">危险文本</el-text>
  <el-text type="info">信息文本</el-text>

  <!-- 截断 -->
  <el-text truncated>这是一段很长的文本会被截断...</el-text>

  <!-- 尺寸 -->
  <el-text size="large">大号</el-text>
  <el-text size="default">默认</el-text>
  <el-text size="small">小号</el-text>

  <!-- 加粗/斜体/删除线/下划线/代码 -->
  <el-text tag="b">加粗</el-text>
  <el-text tag="i">斜体</el-text>
  <el-text tag="del">删除线</el-text>
  <el-text tag="u">下划线</el-text>
  <el-text tag="code">行内代码</el-text>
</template>
```

\---

### 9\. Scrollbar 滚动条

**用途**: 替代原生滚动条，提供统一风格。

```vue
<template>
  <el-scrollbar height="400px">
    <div v-for="item in 50" :key="item" style="padding: 10px;">
      内容 {{ item }}
    </div>
  </el-scrollbar>

  <!-- 水平滚动 -->
  <el-scrollbar>
    <div style="white-space: nowrap; width: 800px;">
      很长的横向内容...
    </div>
  </el-scrollbar>
</template>
```

|Prop|类型|默认值|说明|
|-|-|-|-|
|height|string / number|-|容器高度|
|max-height|string / number|-|最大高度|
|always|boolean|false|始终显示滚动条|

\---

### 10\. Space 间距

**用途**: 设置子元素之间的间距。

```vue
<template>
  <el-space :size="20" wrap>
    <el-card v-for="i in 4" :key="i" style="width: 200px;">卡片 {{ i }}</el-card>
  </el-space>

  <!-- 对齐 -->
  <el-space alignment="center">
    <span>左对齐</span>
    <el-button>中间对齐</el-button>
    <span>右对齐</span>
  </el-space>

  <!-- 方向 -->
  <el-space direction="vertical" :fill="true">
    <div>垂直排列项 1</div>
    <div>垂直排列项 2</div>
  </el-space>
</template>
```

|Prop|类型|默认值|说明|
|-|-|-|-|
|size|number / string|-|间距大小|
|direction|`horizontal / vertical`|horizontal|方向|
|alignment|`start / end / center / baseline / stretch`|start|对齐方式|
|wrap|boolean|false|自动换行|
|fill|boolean|false|子元素撑满宽度|

\---

### 11\. Splitter 分隔面板（v2.10.0+）

**用途**: 可拖拽调整大小的面板分隔器。

```vue
<template>
  <el-splitter>
    <el-splitter-pane>
      <template #default>左侧面板</template>
    </el-splitter-pane>
    <el-splitter-pane>
      <template #default>右侧面板</template>
    </el-splitter-pane>
  </el-splitter>

  <!-- 设置初始比例 -->
  <el-splitter>
    <el-splitter-pane :size="30">30%</el-splitter-pane>
    <el-splitter-pane :size="70">70%</el-splitter-pane>
  </el-splitter>
</template>
```

\---

### 12\. Typography 排版

**用途**: 提供标题、段落、文本等排版样式。

```vue
<template>
  <el-typography>
    <h1>一级标题 h1</h1>
    <h2>二级标题 h2</h2>
    <h3>三级标题 h3</h3>
    <h4>四级标题 h4</h4>
    <h5>五级标题 h5</h5>
    <h6>六级标题 h6</h6>
    <p>正文段落文本内容...</p>
  </el-typography>
</template>
```

\---

## 二、Config 配置组件（1个）

### 13\. Config Provider 全局配置

**用途**: 为所有后代组件提供统一的配置（语言、尺寸、z-index 等），无需全局引入。

```vue
<template>
  <el-config-provider :locale="zhCn" :size="'large'" :button="{ autoInsertSpace: true }">
    <App />
  </el-config-provider>
</template>

<script setup>
import zhCn from 'element-plus/es/locale/lang/zh-cn'
import App from './App.vue'
</script>
```

|Prop|类型|说明|
|-|-|-|
|locale|object|语言包对象|
|size|string|组件默认尺寸|
|button|object|Button 组件默认配置|
|message|object|Message 组件默认配置|

> \*\*常用语言包\*\*: `en`, `zhCn`, `es`, `fr`, `de`, `ja`, `ko`, `pt`, `ru` 等

\---

## 三、Form 表单组件（25个）

### 14\. Autocomplete 自动补全输入框

**用途**: 输入时自动显示匹配建议列表。

```vue
<template>
  <el-autocomplete
    v-model="state"
    :fetch-suggestions="querySearch"
    placeholder="请输入"
    @select="handleSelect"
  />
</template>

<script setup>
import { ref } from 'vue'

const state = ref('')
const querySearch = (queryString, cb) => {
  const results = queryString
    ? restaurants.filter(r => r.value.includes(queryString))
    : restaurants
  cb(results)
}
const handleSelect = item => console.log(item)

const restaurants = \[
  { value: '三全鲜食（北新泾店）', address: '长宁区新渔路144号' },
  { value: 'Hot honey 首尔炸鸡（仙霞路）', address: '长宁区淞虹路661号' },
]
</script>
```

\---

### 15\. Cascader 级联选择器

**用途**: 多级联动选择（如省市区、组织架构）。

```vue
<template>
  <el-cascader
    v-model="value"
    :options="options"
    placeholder="请选择"
    clearable
    filterable
    :props="{ checkStrictly: true }"
  />
</template>

<script setup>
import { ref } from 'vue'

const value = ref(\[])
const options = \[
  {
    value: 'guide',
    label: '指南',
    children: \[
      { value: 'design', label: '设计原则' },
      { value: 'navigation', label: '导航' },
    ],
  },
]
</script>
```

|Props (props)|说明|
|-|-|
|options|数据源|
|props.expandTrigger|`hover / click` 展开触发方式|
|props.checkStrictly|是否可选中任意级别|
|props.multiple|是否多选|
|props.emitPath|返回值是否为完整路径|
|filterable|是否可搜索|
|clearable|是否可清空|

\---

### 16\. Checkbox 多选框

**用途**: 多选项选择。

```vue
<template>
  <!-- 单独使用 -->
  <el-checkbox v-model="checked" label="选项A" />

  <!-- 多选组 -->
  <el-checkbox-group v-model="checkList">
    <el-checkbox label="Option A" value="a" />
    <el-checkbox label="Option B" value="b" />
    <el-checkbox label="Option C" value="c" :disabled="true" />
  </el-checkbox-group>

  <!-- 带 border -->
  <el-checkbox-group v-model="checkList2">
    <el-checkbox label="备选项1" border />
    <el-checkbox label="备选项2" border />
  </el-checkbox-group>
</template>

<script setup>
import { ref } from 'vue'
const checked = ref(true)
const checkList = ref(\['a'])
const checkList2 = ref(\['备选项1'])
</script>
```

\---

### 17\. ColorPickerPanel 颜色选择器面板（v2.11.0+）

**用途**: 无浮层的颜色选择面板，嵌入表单中使用。

```vue
<template>
  <el-color-picker-panel v-model="color" />
</template>

<script setup>
import { ref } from 'vue'
const color = ref('#409EFF')
</script>
```

\---

### 18\. Color Picker 颜色选择器

**用途**: 拾取颜色的交互组件。

```vue
<template>
  <el-color-picker v-model="color" show-alpha />
  <el-color-picker v-model="color2" :predefine="\['#409EFF', '#67C23A']" />
</template>

<script setup>
import { ref } from 'vue'
const color = ref('#409EFF')
const color2 = ref('')
</script>
```

|Prop|类型|默认值|说明|
|-|-|-|-|
|modelValue|string|-|绑定值|
|show-alpha|boolean|false|支持透明度|
|color-format|`hsl / hsv / hex / rgb`|hex|颜色格式|
|predefine|string\[]|-|预设颜色|
|disabled|boolean|false|禁用|
|size|string|-|尺寸|

\---

### 19\. Date Picker Panel 日期选择器面板（v2.11.0+）

**用途**: 无浮层的日期面板，用于内嵌场景。

```vue
<template>
  <el-date-picker-panel v-model="date" type="date" />
</template>
```

\---

### 20\. Date Picker 日期选择器

**用途**: 选择日期或日期范围。

```vue
<template>
  <!-- 选择单个日期 -->
  <el-date-picker v-model="date" type="date" placeholder="选择日期" />

  <!-- 日期范围 -->
  <el-date-picker
    v-model="range"
    type="daterange"
    range-separator="至"
    start-placeholder="开始日期"
    end-placeholder="结束日期"
  />

  <!-- 月份选择 -->
  <el-date-picker v-model="month" type="month" placeholder="选择月" />

  <!-- 年份选择 -->
  <el-date-picker v-model="year" type="year" placeholder="选择年" />

  <!-- 多个日期 -->
  <el-date-picker v-model="dates" type="dates" placeholder="选择多个日期" />
</template>

<script setup>
import { ref } from 'vue'
const date = ref('')
const range = ref('')
const month = ref('')
const year = ref('')
const dates = ref(\[])
</script>
```

|type 可选值|说明|
|-|-|
|date|单日|
|daterange|日期范围|
|month|月|
|year|年|
|dates|多个日期|
|datetime|日期时间|
|datetimerange|日期时间范围|
|week|周|

\---

### 21\. DateTime Picker 日期时间选择器

**用途**: 同时选择日期和时间。

```vue
<template>
  <el-date-picker
    v-model="datetime"
    type="datetime"
    placeholder="选择日期时间"
    format="YYYY-MM-DD HH:mm:ss"
    value-format="YYYY-MM-DD HH:mm:ss"
  />
</template>

<script setup>
import { ref } from 'vue'
const datetime = ref('')
</script>
```

\---

### 22\. Form 表单组件

**用途**: 表单数据收集与校验的容器。

```vue
<template>
  <el-form
    ref="formRef"
    :model="form"
    :rules="rules"
    label-width="120px"
    status-icon
  >
    <el-form-item label="活动名称" prop="name">
      <el-input v-model="form.name" />
    </el-form-item>
    <el-form-item label="活动区域" prop="region">
      <el-select v-model="form.region" placeholder="请选择">
        <el-option label="区域一" value="shanghai" />
        <el-option label="区域二" value="beijing" />
      </el-select>
    </el-form-item>
    <el-form-item label="即时配送" prop="delivery">
      <el-switch v-model="form.delivery" />
    </el-form-item>
    <el-form-item>
      <el-button type="primary" @click="submitForm(formRef)">提交</el-button>
      <el-button @click="resetForm(formRef)">重置</el-button>
    </el-form-item>
  </el-form>
</template>

<script lang="ts" setup>
import { reactive, ref } from 'vue'
import type { FormInstance, FormRules } from 'element-plus'

const formRef = ref<FormInstance>()
const form = reactive({
  name: '',
  region: '',
  delivery: false,
})

const rules = reactive<FormRules>({
  name: \[
    { required: true, message: '请输入活动名称', trigger: 'blur' },
    { min: 3, max: 5, message: '长度在 3 到 5 个字符', trigger: 'blur' },
  ],
  region: \[{ required: true, message: '请选择活动区域', trigger: 'change' }],
})

const submitForm = async (formEl: FormInstance | undefined) => {
  if (!formEl) return
  await formEl.validate((valid) => {
    if (valid) console.log('提交!', form)
  })
}

const resetForm = (formEl: FormInstance | undefined) => {
  if (!formEl) return
  formEl.resetFields()
}
</script>
```

|Form Props|说明|
|-|-|
|model|表单数据对象|
|rules|校验规则|
|label-width|标签宽度|
|label-position|`right / left / top` 标签位置|
|inline|行内模式|
|disabled|整表禁用|
|status-icon|显示校验结果图标|
|require-asterisk-position|`left / right` 必填星号位置|

|FormItem Props|说明|
|-|-|
|prop|字段名（对应 model 和 rules）|
|label|标签文字|
|required|是否必填|
|rules|单独的校验规则|
|error|错误信息|
|show-message|是否显示错误信息|

\---

### 23\. Input 输入框

**用途**: 文本输入基础组件。

```vue
<template>
  <!-- 基础用法 -->
  <el-input v-model="input" placeholder="请输入内容" />

  <!-- 可清空 -->
  <el-input v-model="input2" clearable />

  <!-- 密码框 -->
  <el-input v-model="password" type="password" show-password />

  <!-- 文本域 -->
  <el-input v-model="textarea" type="textarea" :rows="3" />

  <!-- 复合型输入框 -->
  <el-input v-model="search" placeholder="请搜索">
    <template #prefix><el-icon><Search /></el-icon></template>
  </el-input>

  <el-input v-model="url" placeholder="请输入网址">
    <template #prepend>https://</template>
    <template #append>.com</template>
  </el-input>

  <!-- 输入长度限制 -->
  <el-input v-model="text" maxlength="10" show-word-limit />
</template>
```

|Prop|类型|默认值|说明|
|-|-|-|-|
|modelValue|string / number|-|绑定值|
|type|`text / textarea / password / url / email / date / number`|text|类型|
|maxlength|number|-|最大长度|
|minlength|number|-|最小长度|
|show-word-limit|boolean|false|显示字数统计|
|placeholder|string|-|占位符|
|clearable|boolean|false|可清空|
|show-password|boolean|false|切换密码可见|
|disabled|boolean|false|禁用|
|size|string|-|尺寸|
|prefix-icon / suffix-icon|string / Component|-|头部/尾部图标|
|rows|number|2|文本域行数|
|autosize|boolean / object|false|自适应高度|

|Event|说明|
|-|-|
|blur|失去焦点|
|focus|获得焦点|
|change|值改变|
|input|输入时|
|clear|清空|

\---

### 24\. Input Number 数字输入框

**用途**: 只能输入数字的输入框，支持步进控制。

```vue
<template>
  <el-input-number v-model="num" :min="1" :max="10" />
  <el-input-number v-model="num2" :step="2" step-strictly />
  <el-input-number v-model="num3" controls-position="right" />
  <el-input-number v-model="num4" disabled />
</template>

<script setup>
import { ref } from 'vue'
const num = ref(1)
const num2 = ref(0)
const num3 = ref(0)
const num4 = ref(0)
</script>
```

|Prop|说明|
|-|-|
|min / max|最小/最大值|
|step|步长|
|step-strictly|只能输入步长的倍数|
|precision|精度（小数位数）|
|controls-position|`right` 按钮位置|
|disabled|禁用|

\---

### 25\. Input Tag 标签输入框（v2.9.0+）

**用途**: 输入并生成标签（Tag）。

```vue
<template>
  <el-input-tag v-model="tags" placeholder="输入后回车添加标签" />
</template>

<script setup>
import { ref } from 'vue'
const tags = ref(\['标签1', '标签2'])
</script>
```

\---

### 26\. Input OTP（v2.14.0+）

**用途**: 验证码/一次性密码输入。

```vue
<template>
  <el-input-otp v-model="otp" length="6" />
</template>

<script setup>
import { ref } from 'vue'
const otp = ref('')
</script>
```

\---

### 27\. Mention 提及（v2.8.0+）

**用途**: @提及功能，常用于评论、聊天场景。

```vue
<template>
  <el-mention
    v-model="value"
    :options="options"
    placeholder="输入 @ 提及用户"
  />
</template>

<script setup>
import { ref } from 'vue'

const value = ref('')
const options = \[
  { value: '张三', label: '张三' },
  { value: '李四', label: '李四' },
  { value: '王五', label: '王五' },
]
</script>
```

\---

### 28\. Radio 单选框

**用途**: 单选项选择。

```vue
<template>
  <!-- 单独使用 -->
  <el-radio v-model="radio" label="1">选项 A</el-radio>
  <el-radio v-model="radio" label="2">选项 B</el-radio>

  <!-- 单选组 -->
  <el-radio-group v-model="radio2">
    <el-radio value="1">选项 A</el-radio>
    <el-radio value="2">选项 B</el-radio>
    <el-radio value="3" disabled>禁用</el-radio>
  </el-radio-group>

  <!-- 按钮样式 -->
  <el-radio-group v-model="radio3">
    <el-radio-button value="上海">上海</el-radio-button>
    <el-radio-button value="北京">北京</el-radio-button>
    <el-radio-button value="广州">广州</el-radio-button>
  </el-radio-group>
</template>

<script setup>
import { ref } from 'vue'
const radio = ref('1')
const radio2 = ref('1')
const radio3 = ref('上海')
</script>
```

\---

### 29\. Rate 评分

**用途**: 星级评分组件。

```vue
<template>
  <el-rate v-model="rate" />
  <el-rate v-model="rate2" allow-half />
  <el-rate v-model="rate3" :texts="\['差', '一般', '好', '很好', '极好']" show-text />
  <el-rate v-model="rate4" :icons="\[Star, StarFilled, StarFilled]" />
</template>

<script setup>
import { ref } from 'vue'
import { Star, StarFilled } from '@element-plus/icons-vue'
const rate = ref(null)
const rate2 = ref(null)
const rate3 = ref(null)
const rate4 = ref(null)
</script>
```

|Prop|说明|
|-|-|
|max|最大分数（默认 5）|
|allow-half|允许半星|
|allow-clear|允许再次点击取消评分|
|texts|辅助文字数组|
|show-text|显示辅助文字|
|colors|自定义颜色数组|
|icons|自定义图标数组|

\---

### 30\. Select 选择器

**用途**: 下拉选项选择。

```vue
<template>
  <el-select v-model="value" placeholder="请选择" clearable filterable>
    <el-option
      v-for="item in options"
      :key="item.value"
      :label="item.label"
      :value="item.value"
      :disabled="item.disabled"
    />
  </el-select>

  <!-- 分组 -->
  <el-select v-model="groupValue" placeholder="分组选择">
    <el-option-group label="热门城市">
      <el-option label="上海" value="shanghai" />
      <el-option label="北京" value="beijing" />
    </el-option-group>
    <option-group label="城市名">
      <el-option label="成都" value="chengdu" />
    </option-group>
  </el-select>

  <!-- 多选 -->
  <el-select v-model="multiValue" multiple collapse-tags placeholder="多选">
    <el-option label="选项1" value="1" />
    <el-option label="选项2" value="2" />
  </el-select>
</template>

<script setup>
import { ref } from 'vue'
const value = ref('')
const groupValue = ref('')
const multiValue = ref(\[])

const options = \[
  { value: '1', label: '黄金糕' },
  { value: '2', label: '双皮奶' },
  { value: '3', label: '蚵仔煎', disabled: true },
]
</script>
```

|Prop|说明|
|-|-|
|multiple|多选|
|filterable|可搜索|
|remote|远程搜索|
|remote-method|远程搜索方法|
|clearable|可清空|
|collapse-tags|多选折叠|
|collapse-tags-tooltip|折叠后 hover 显示 tooltip|
|allow-create|允许创建新条目|
|default-first-option|回车选中第一个|
|reserve-keyword|关键词保留|
|loading|加载状态|
|no-match-text|无匹配文本|
|no-data-text|无数据文本|
|popper-class|下拉菜单自定义类名|
|fit-input-width|下拉菜单宽度自适应|

\---

### 31\. Virtualized Select 虚拟化选择器

**用途**: 大数据量下拉选择，使用虚拟滚动优化性能。

```vue
<template>
  <el-select-v2
    v-model="value"
    :options="largeOptions"
    placeholder="请选择"
    style="width: 240px"
  />
</template>

<script setup>
import { ref } from 'vue'

const value = ref('')
const largeOptions = Array.from({ length: 10000 }).map((\_, idx) => ({
  value: `option-${idx}`,
  label: `选项 ${idx}`,
}))
</script>
```

> \*\*适用场景\*\*: 选项数量 > 1000 时使用，避免渲染卡顿。

\---

### 32\. Slider 滑块

**用途**: 拖拽滑块选择数值。

```vue
<template>
  <el-slider v-model="value" />
  <el-slider v-model="value2" :step="10" show-stops show-input />
  <el-slider v-model="range" range :min="0" :max="100" />
  <el-slider v-model="value3" vertical height="200px" />
</template>

<script setup>
import { ref } from 'vue'
const value = ref(0)
const value2 = ref(0)
const range = ref(\[20, 80])
const value3 = ref(0)
</script>
```

|Prop|说明|
|-|-|
|min / max|最小/最大值|
|step|步长|
|range|范围模式|
|show-input|显示输入框|
|show-stops|显示间断点|
|show-tooltip|显示 tooltip|
|format-tooltip|自定义 tooltip 格式化|
|vertical|垂直方向|
|marks|标记（对象 `{0: '0°C', 50: '50°C'}`）|
|disabled|禁用|

\---

### 33\. Switch 开关

**用途**: 切换开关状态。

```vue
<template>
  <el-switch v-model="value" />
  <el-switch v-model="value2" active-text="开启" inactive-text="关闭" />
  <el-switch v-model="value3" active-color="#13ce66" inactive-color="#ff4949" />
  <el-switch v-model="value4" disabled />
  <el-switch v-model="value5" :active-value="100" :inactive-value="0" />
</template>

<script setup>
import { ref } from 'vue'
const value = ref(true)
const value2 = ref(true)
const value3 = ref(true)
const value4 = ref(false)
const value5 = ref(100)
</script>
```

|Prop|说明|
|-|-|
|model-value|绑定值|
|active-value / inactive-value|开关时的值（支持 string/number/boolean）|
|active-color / inactive-color|开关颜色|
|active-text / inactive-text|开关文字|
|disabled|禁用|
|loading|加载状态|
|size|尺寸|
|inline-prompt|内嵌文字|
|before-change|切换前的钩子函数（返回 Promise）|

\---

### 34\. Time Picker 时间选择器

**用途**: 选择具体时间（时:分:秒）。

```vue
<template>
  <el-time-picker v-model="time" placeholder="选择时间" />
  <el-time-picker
    v-model="timeRange"
    is-range
    range-separator="至"
    start-placeholder="开始时间"
    end-placeholder="结束时间"
  />
</template>

<script setup>
import { ref } from 'vue'
const time = ref('')
const timeRange = ref('')
</script>
```

\---

### 35\. Time Select 时间选择

**用途**: 固定时间点选择（非任意时间）。

```vue
<template>
  <el-time-select
    v-model="value"
    start="08:30"
    step="00:15"
    end="18:30"
    placeholder="选择时间"
  />
</template>

<script setup>
import { ref } from 'vue'
const value = ref('')
</script>
```

\---

### 36\. Transfer 穿梭框

**用途**: 左右穿梭选择（源列表 ↔ 目标列表）。

```vue
<template>
  <el-transfer
    v-model="value"
    :data="data"
    :titles="\['源列表', '目标列表']"
    filterable
    filter-placeholder="搜索"
  />
</template>

<script setup>
import { ref } from 'vue'

const generateData = () => {
  const data = \[]
  for (let i = 1; i <= 15; i++) {
    data.push({ key: i, label: `选项 ${i}`, disabled: i % 4 === 0 })
  }
  return data
}

const data = generateData()
const value = ref(\[1, 4])
</script>
```

|Prop|说明|
|-|-|
|data|数据源（需 key + label）|
|v-model|目标列表 key 数组|
|titles|标题数组|
|filterable|可搜索|
|target-order|`original / push` 目标排序|
|left-default-checked / right-default-checked|默认选中|
|props|字段映射（key/label/disabled）|

\---

### 37\. TreeSelect 树形选择（v2.1.8+）

**用途**: 树形结构的选择器。

```vue
<template>
  <el-tree-select
    v-model="value"
    :data="data"
    check-strictly
    render-after-expand
    placeholder="请选择"
  />
</template>

<script setup>
import { ref } from 'vue'

const value = ref('')
const data = \[
  {
    value: '1',
    label: 'Level one 1',
    children: \[
      { value: '1-1', label: 'Level two 1-1' },
      { value: '1-2', label: 'Level two 1-2' },
    ],
  },
]
</script>
```

\---

### 38\. Upload 上传器

**用途**: 文件上传组件。

```vue
<template>
  <!-- 点击上传 -->
  <el-upload
    action="/api/upload"
    :on-preview="handlePreview"
    :on-remove="handleRemove"
    :before-upload="beforeUpload"
    :file-list="fileList"
  >
    <el-button type="primary">点击上传</el-button>
    <template #tip>
      <div class="el-upload\_\_tip">只能上传 jpg/png 文件，且不超过 500kb</div>
    </template>
  </el-upload>

  <!-- 拖拽上传 -->
  <el-upload
    action="/api/upload"
    drag
    multiple
  >
    <el-icon class="el-icon--upload"><UploadFilled /></el-icon>
    <div class="el-upload\_\_text">将文件拖到此处，或<em>点击上传</em></div>
  </el-upload>

  <!-- 头像上传 -->
  <el-upload
    class="avatar-uploader"
    action="/api/upload"
    :show-file-list="false"
    :on-success="handleAvatarSuccess"
    :before-upload="beforeAvatarUpload"
  >
    <img v-if="imageUrl" :src="imageUrl" class="avatar" />
    <el-icon v-else class="avatar-uploader-icon"><Plus /></el-icon>
  </el-upload>
</template>

<script setup>
import { ref } from 'vue'
import { Plus, UploadFilled } from '@element-plus/icons-vue'

const fileList = ref(\[])
const imageUrl = ref('')

const handlePreview = file => console.log(file)
const handleRemove = (file, fileList) => console.log(fileList)
const beforeUpload = rawFile => {
  if (rawFile.size > 500000) {
    ElMessage.error('文件不能超过 500KB!')
    return false
  }
  return true
}

const handleAvatarSuccess = (response, uploadFile) => {
  imageUrl.value = URL.createObjectURL(uploadFile.raw!)
}
const beforeAvatarUpload = rawFile => {
  if (!\['image/jpeg', 'image/png'].includes(rawFile.type)) {
    ElMessage.error('头像图片只能是 JPG/PNG 格式!')
    return false
  }
  return true
}
</script>
```

|Prop|说明|
|-|-|
|action|上传地址|
|headers|请求头|
|method|POST / PUT|
|data|附带参数|
|name|文件字段名|
|with-credentials|携带 cookie|
|show-file-list|显示文件列表|
|drag|拖拽上传|
|accept|接受的文件类型|
|limit|最大上传数|
|on-exceed|超出限制回调|
|list-type|`text / picture / picture-card`|
|auto-upload|自动上传|
|http-request|自定义上传方法|

\---

## 四、Data 数据展示（23个）

### 39\. Avatar 头像

**用途**: 用户头像或图标展示。

```vue
<template>
  <el-avatar :size="50" src="https://example.com/avatar.png" />
  <el-avatar :size="40">用户</el-avatar>
  <el-avatar shape="square" :size="50" :src="url" />
  <el-avatar :icon="UserFilled" />
</template>
```

|Prop|说明|
|-|-|
|size|尺寸（number 或 large/default/small）|
|shape|`circle / square` 形状|
|src|图片地址|
|alt|替代文本|
|icon|图标组件|
|fit|图片适应方式|

\---

### 40\. Badge 徽章

**用途**: 在角落显示数字/状态标记。

```vue
<template>
  <el-badge :value="99" :max="99">
    <el-button>消息</el-button>
  </el-badge>
  <el-badge is-dot>
    <span class="dot-example">查询</span>
  </el-badge>
  <el-badge value="new" type="primary">
    <el-button>评论</el-button>
  </el-badge>
  <el-badge value="hot" type="danger">
    <el-button>回复</el-button>
  </el-badge>
</template>
```

|Prop|说明|
|-|-|
|value|显示值|
|max|最大值（超过显示 max+）|
|is-dot|小圆点|
|hidden|隐藏|
|type|`primary / success / warning / danger / info`|

\---

### 41\. Calendar 日历

**用途**: 日历展示。

```vue
<template>
  <el-calendar v-model="date">
    <template #date-cell="{ data }">
      <p>{{ data.date.getDate() }}</p>
      <p v-if="isSpecial(data.date)">🎂</p>
    </template>
  </el-calendar>
</template>

<script setup>
import { ref } from 'vue'
const date = ref(new Date())
const isSpecial = date => date.getDate() === 3
</script>
```

\---

### 42\. Card 卡片

**用途**: 内容卡片容器。

```vue
<template>
  <el-card shadow="hover" header="卡片标题">
    卡片内容区域
  </el-card>

  <!-- 简洁卡片 -->
  <el-card>
    <template #header>
      <div class="card-header">
        <span>卡片名称</span>
        <el-button text>操作按钮</el-button>
      </div>
    </template>
    <div v-for="o in 4" :key="o" class="text item">{{ '列表内容 ' + o }}</div>
  </el-card>
</template>
```

|Prop|说明|
|-|-|
|header|卡片标题|
|body-style|body 样式|
|shadow|`always / hover / never` 阴影|

\---

### 43\. Carousel 走马灯

**用途**: 轮播图/轮播内容。

```vue
<template>
  <el-carousel height="150px" indicator-position="outside">
    <el-carousel-item v-for="item in 4" :key="item">
      <h3>{{ item }}</h3>
    </el-carousel-item>
  </el-carousel>

  <!-- 卡片化轮播 -->
  <el-carousel type="card" height="200px">
    <el-carousel-item v-for="i in 6" :key="i">
      <div class="card-panel">{{ i }}</div>
    </el-carousel-item>
  </el-carousel>
</template>
```

|Prop|说明|
|-|-|
|height|高度|
|initial-index|初始索引|
|trigger|`click / hover` 切换触发|
|autoplay|自动播放|
|interval|间隔（ms）|
|indicator-position|`outside / none` 指示器位置|
|arrow|`always / hover / never` 箭头|
|type|`card` 卡片化|
|loop|循环播放|

\---

### 44\. Collapse 折叠面板

**用途**: 可折叠的内容区域。

```vue
<template>
  <el-collapse v-model="activeNames">
    <el-collapse-item title="一致性 Consistency" name="1">
      <div>与现实生活一致：与现实生活的流程、逻辑保持一致...</div>
    </el-collapse-item>
    <el-collapse-item title="反馈 Feedback" name="2">
      <div>控制反馈：通过界面样式和交互动效让用户可以清晰的感知自己的操作...</div>
    </el-collapse-item>
  </el-collapse>

  <!-- 手风琴模式（同时只展开一个） -->
  <el-collapse accordion>
    <el-collapse-item title="面板 A" name="a">内容 A</el-collapse-item>
    <el-collapse-item title="面板 B" name="b">内容 B</el-collapse-item>
  </el-collapse>
</template>

<script setup>
import { ref } from 'vue'
const activeNames = ref(\['1'])
</script>
```

\---

### 45\. Descriptions 描述列表

**用途**: 展示键值对列表（如详情页信息）。

```vue
<template>
  <el-descriptions title="用户信息" :column="2" border>
    <el-descriptions-item label="用户名">kooriookami</el-descriptions-item>
    <el-descriptions-item label="手机号">18100000000</el-descriptions-item>
    <el-descriptions-item label="居住地">苏州市</el-descriptions-item>
    <el-descriptions-item label="备注">
      <el-tag size="small">学校</el-tag>
    </el-descriptions-item>
    <el-descriptions-item label="联系地址">
      江苏省苏州市吴中区吴中大道 1188 号
    </el-descriptions-item>
  </el-descriptions>
</template>
```

|Prop|说明|
|-|-|
|title|标题|
|column|列数|
|border|带边框|
|direction|`vertical / horizontal` 排列方向|
|size|尺寸|
|extra|右侧操作区|

\---

### 46\. Empty 空状态

**用途**: 数据为空时的占位提示。

```vue
<template>
  <el-empty description="暂无数据" />
  <el-empty :image-size="200" description="自定义描述文案">
    <el-button type="primary">去添加</el-button>
  </el-empty>
</template>
```

|内置 image 类型|说明|
|-|-|
|默认|无数据|
|error|错误页面|
|search|搜索无结果|

\---

### 47\. Image 图片

**用途**: 图片展示（含预览、懒加载、错误处理等）。

```vue
<template>
  <el-image
    src="https://example.com/image.png"
    fit="cover"
    :preview-src-list="srcList"
    lazy
  >
    <template #error>
      <div class="image-slot">
        <el-icon><PictureFilled /></el-icon>
      </div>
    </template>
  </el-image>
</template>

<script setup>
const srcList = \['https://example.com/image.png']
</script>
```

|Prop|说明|
|-|-|
|src|图片地址|
|fit|`fill / contain / cover / none / scale-down`|
|alt|替代文本|
|preview-src-list|预览图片列表|
|lazy|懒加载|
|scroll-container|懒加载容器|

\---

### 48\. Infinite Scroll 无限滚动

**用途**: 滚动到底部自动加载更多数据。

```vue
<template>
  <ul v-infinite-scroll="load" class="infinite-list" infinite-scroll-distance="50">
    <li v-for="i in count" :key="i" class="infinite-list-item">{{ i }}</li>
  </ul>
</template>

<script setup>
import { ref } from 'vue'

const count = ref(0)
const load = () => {
  count.value += 2
}
</script>
```

|Directive Options|说明|
|-|-|
|v-infinite-scroll|触发方法|
|infinite-scroll-disabled|是否禁用|
|infinite-scroll-delay|延迟（ms）|
|infinite-scroll-distance|距离底部距离（px）|
|infinite-scroll-immediate|是否立即执行|

\---

### 49\. Pagination 分页

**用途**: 数据分页导航。

```vue
<template>
  <el-pagination
    v-model:current-page="currentPage"
    v-model:page-size="pageSize"
    :page-sizes="\[10, 20, 50, 100]"
    :total="total"
    layout="total, sizes, prev, pager, next, jumper"
    @size-change="handleSizeChange"
    @current-change="handleCurrentChange"
  />
</template>

<script setup>
import { ref } from 'vue'

const currentPage = ref(1)
const pageSize = ref(10)
const total = ref(1000)

const handleSizeChange = val => console.log(`每页 ${val} 条`)
const handleCurrentChange = val => console.log(`当前页 ${val}`)
</script>
```

|Prop|说明|
|-|-|
|total|总条数|
|page-size|每页条数|
|current-page|当前页码|
|page-sizes|每页条数选项|
|layout|布局（如 `sizes, prev, pager, next, jumper, ->, total`）|
|background|背景色|
|small|小型|
|hide-on-single-page|只有一页时隐藏|

\---

### 50\. Progress 进度条

**用途**: 任务进度展示。

```vue
<template>
  <el-progress :percentage="50" />
  <el-progress :percentage="100" status="success" />
  <el-progress :percentage="70" color="#e6a23c" />
  <el-progress :percentage="60" :stroke-width="26" :text-inside="true" />
  <el-progress type="circle" :percentage="25" />
  <el-progress type="dashboard" :percentage="80" />
  <el-progress :percentage="percentage" :color="customColor" />
</template>

<script setup>
import { ref } from 'vue'
const percentage = ref(20)
const customColor = (percentage) =>
  percentage < 20 ? '#f56c6c' : percentage < 50 ? '#e6a23c' : '#67c23a'
</script>
```

|Prop|说明|
|-|-|
|percentage|百分比|
|status|`success / exception / warning / text`|
|stroke-width|宽度|
|text-inside|文字在内|
|type|`line / circle / dashboard / bar`|
|color|颜色（string 或 function）|
|duration|动画时长|
|striped|条纹动画|
|indeterminate|不确定进度|

\---

### 51\. Result 结果页

**用途**: 操作结果反馈页（成功/失败/警告/信息/404/403/500）。

```vue
<template>
  <el-result icon="success" title="操作成功" sub-title="请按照提示进行操作">
    <template #extra>
      <el-button type="primary">返回</el-button>
    </template>
  </el-result>

  <el-result icon="warning" title="操作警告" sub-title="请注意相关事项" />
  <el-result icon="error" title="错误提示" sub-title="请联系管理员" />
  <el-result icon="info" title="信息提示" />
  <el-result icon="404" title="404" sub-title="抱歉，您访问的页面不存在" />
</template>
```

| icon 可选值 | `success / warning / error / info / 404 / 403 / 500` |

\---

### 52\. Skeleton 骨架屏

**用途**: 加载时的占位骨架动画。

```vue
<template>
  <el-skeleton :rows="5" animated />

  <!-- 自定义模板 -->
  <el-skeleton :loading="loading" animated template="<div class='custom-template'>...</div>">
    <template #template>
      <el-skeleton-item variant="circle" style="width: 50px; height: 50px;" />
      <el-skeleton-item variant="h1" style="width: 50%;" />
      <el-skeleton-item variant="rect" style="width: 100%; height: 20px;" />
    </template>
    <template #default>
      <div>实际内容</div>
    </template>
  </el-skeleton>
</template>
```

| skeleton-item variant | `text / h1 / h3 / caption / p / image / rect / circle / button` |

\---

### 53\. Table 表格

**用途**: 强大的数据表格（排序、筛选、分页、展开、树形等）。

```vue
<template>
  <el-table :data="tableData" stripe border style="width: 100%">
    <el-table-column prop="date" label="Date" width="180" sortable />
    <el-table-column prop="name" label="Name" width="180" />
    <el-table-column prop="address" label="Address" />

    <!-- 自定义列模板 -->
    <el-table-column label="Operations" fixed="right" width="200">
      <template #default="scope">
        <el-button size="small" @click="handleEdit(scope.$index, scope.row)"
          >编辑</el-button
        >
        <el-button
          size="small"
          type="danger"
          @click="handleDelete(scope.$index, scope.row)"
          >删除</el-button
        >
      </template>
    </el-table-column>

    <!-- 多选 -->
    <el-table-column type="selection" width="55" />

    <!-- 序号 -->
    <el-table-column type="index" width="50" />

    <!-- 展开 -->
    <el-table-column type="expand">
      <template #default="scope">
        <div>{{ scope.row.detail }}</div>
      </template>
    </el-table-column>
  </el-table>
</template>

<script setup>
const tableData = \[
  { date: '2016-05-03', name: 'Tom', address: 'No. 189, Grove St., Los Angeles' },
  { date: '2016-05-02', name: 'Jack', address: 'No. 189, Grove St., Los Angeles' },
]

const handleEdit = (index, row) => console.log(index, row.name)
const handleDelete = (index, row) => console.log(index, row.name)
</script>
```

#### Table 核心 Props

|Prop|说明|
|-|-|
|data|数据数组|
|stripe|斑马纹|
|border|边框|
|height / max-height|固定表头|
|row-key|行数据的 Key|
|default-expand-all|默认展开所有|
|tree-props|树形数据配置|
|highlight-current-row|当前行高亮|
|empty-text|空数据文本|
|show-summary|显示合计行|
|summary-method|合计方法|
|span-method|合并单元格方法|
|cell-class-name / row-class-name|自定义类名|
|lazy|懒加载|
|load|懒加载方法|
|default-sort|默认排序|
|scrollbar-always-on|始终显示滚动条|

#### TableColumn 核心 Props

|Prop|说明|
|-|-|
|prop|字段名|
|label|列标题|
|width / min-width|列宽|
|fixed|`left / right / true` 固定列|
|sortable|`true / custom / 'descending' / 'ascending'` 排序|
|filters|筛选条件|
|filter-method|筛选方法|
|formatter|格式化函数|
|align|对齐方式|
|show-overflow-tooltip|内容过长 tooltip|
|type|`selection / index / expand` 特殊列|
|resizable|可拖拽调整宽度|

#### Table Events

|Event|参数|说明|
|-|-|-|
|select|selection, row|手动勾选|
|select-all|selection|全选|
|selection-change|selection|选中变化|
|sort-change|{ column, prop, order }|排序变化|
|row-click|row, column, event|行点击|
|row-dblclick|row, column, event|行双击|
|current-change|currentRow, oldCurrentRow|当前行变化|
|expand-change|row, expandedRows|展开变化|

#### Table Methods（通过 ref 调用）

|Method|说明|
|-|-|
|clearSelection()|清空选中|
|toggleRowSelection(row, selected)|切换行选中|
|toggleAllSelection()|切换全选|
|setCurrentRow(row)|设置当前行|
|clearSort()|清除排序|
|sort(prop, order)|手动排序|
|setData(data)|设置数据|

\---

### 54\. Virtualized Table 虚拟化表格（v2.2.0+）

**用途**: 大数据量表格，虚拟滚动优化性能。

```vue
<template>
  <el-table-v2
    :columns="columns"
    :data="data"
    :width="700"
    :height="400"
    fixed
  />
</template>

<script setup>
const columns = \[
  { key: 'id', dataKey: 'id', title: 'ID', width: 100 },
  { key: 'name', dataKey: 'name', title: 'Name', width: 200 },
  { key: 'date', dataKey: 'date', title: 'Date', width: 200 },
]
const data = Array.from({ length: 10000 }).map((\_, i) => ({ id: i, name: `Name ${i}`, date: new Date().toISOString() }))
</script>
```

> \*\*适用场景\*\*: 数据行数 > 1000 时使用。

\---

### 55\. Tag 标签

**用途**: 标记和分类标签。

```vue
<template>
  <el-tag>标签一</el-tag>
  <el-tag type="success">标签二</el-tag>
  <el-tag type="info">标签三</el-tag>
  <el-tag type="warning">标签四</el-tag>
  <el-tag type="danger">标签五</el-tag>

  <!-- 可关闭 -->
  <el-tag closable @close="handleClose">可关闭标签</el-tag>

  <!-- 不同尺寸 -->
  <el-tag effect="dark">暗黑效果</el-tag>
  <el-tag effect="plain">朴素效果</el-tag>

  <el-tag size="large">大号</el-tag>
  <el-tag size="default">默认</el-tag>
  <el-tag size="small">小号</el-tag>
</template>
```

|Prop|说明|
|-|-|
|type|`success / info / warning / danger`|
|closable|可关闭|
|disable-transitions|禁用渐变动画|
|hit|有边框描边|
|color|背景色|
|effect|`dark / plain / light`|
|size|`large / default / small`|
|round|圆角|

\---

### 56\. Timeline 时间线

**用途**: 垂直时间线展示。

```vue
<template>
  <el-timeline>
    <el-timeline-item timestamp="2018/4/12" placement="top">
      <el-card>
        <h4>更新 Github 模板</h4>
        <p>王小虎 提交于 2018/4/12 20:46</p>
      </el-card>
    </el-timeline-item>
    <el-timeline-item timestamp="2018/4/3" placement="top">
      <el-card>
        <h4>更新 Github 模板</h4>
        <p>王小虎 提交于 2018/4/3 20:46</p>
      </el-card>
    </el-timeline-item>
    <el-timestamp color="#0bbd87">最后一个节点</el-timestamp>
  </el-timeline>
</template>
```

|Prop|说明|
|-|-|
|timestamp|时间戳|
|placement|`top / bottom` 内容位置|
|hide-timestamp|隐藏时间戳|
|center|居中对齐|
|color|颜色/圆点类型|
|size|`normal / large`|
|hollow|空心圆点|
|type|`primary / success / warning / danger / info`|

\---

### 57\. Tour 漫游式引导（v2.5.0+）

**用途**: 新手引导/功能指引。

```vue
<template>
  <el-tour :steps="steps">
    <template #default="{ current, index, isFirst, isLast, prev, next }">
      <el-button @click="next">开始引导</el-button>
    </template>
  </el-tour>
</template>

<script setup>
const steps = \[
  { title: '注册', description: '在这里注册你的账号', target: '.register-btn' },
  { title: '登录', description: '登录你的账户', target: '.login-btn' },
]
</script>
```

\---

### 58\. Tree 树形控件

**用途**: 树形结构展示与交互。

```vue
<template>
  <el-tree
    :data="data"
    :props="defaultProps"
    node-key="id"
    default-expand-all
    @node-click="handleNodeClick"
    show-checkbox
    :expand-on-click-node="false"
  />
</template>

<script setup>
const defaultProps = {
  children: 'children',
  label: 'label',
}

const data = \[
  {
    id: 1,
    label: 'Level one 1',
    children: \[
      { id: 4, label: 'Level two 1-1', children: \[] },
      { id: 5, label: 'Level two 1-2', children: \[] },
    ],
  },
]

const handleNodeClick = data => console.log(data)
</script>
```

|Prop|说明|
|-|-|
|data|树形数据|
|props|配置选项（children/label/isLeaf/disabled）|
|node-key|每个节点的唯一标识|
|default-expanded-keys|默认展开节点|
|default-checked-keys|默认选中节点|
|current-node-key|当前选中节点|
|show-checkbox|显示复选框|
|check-strictly|父子不关联|
|draggable|可拖拽|
|lazy|懒加载|
|load|懒加载方法|
|filter-node-method|过滤方法|
|highlight-current|高亮当前节点|
|accordion|手风琴模式|
|indent|缩进|
|icon-class|自定义图标类名|

\---

### 59\. Virtualized Tree 虚拟化树形控件

**用途**: 大数据量树形控件，虚拟滚动优化。

```vue
<template>
  <el-tree-v2
    :data="data"
    :props="props"
    :height="300"
    :item-size="28"
  />
</template>
```

> \*\*适用场景\*\*: 树节点数量 > 1000 时使用。

\---

### 60\. Statistic 统计组件（v2.2.30+）

**用途**: 数值统计展示。

```vue
<template>
  <el-statistic title="Active Users" :value="268500" />
  <el-statistic :value="98500" title="年销售额（元）" :precision="2">
    <template #suffix>
      <el-icon style="vertical-align: middle"><ChatLineRound /></el-icon>
    </template>
  </el-statistic>
  <el-statistic title="倒计时" :value="deadline" time-indices style="margin-right: 50px" />
</template>
```

|Prop|说明|
|-|-|
|value|数值|
|title|标题|
|precision|精度|
|suffix / prefix|后缀/前缀插槽|
|value-style|数值样式|
|decimal-separator|小数点分隔符|
|group-separator|千分位分隔符|
|time-indices|倒计时模式|

\---

### 61\. Segmented 分段控制器（v2.7.0+）

**用途**: 分段切换控制。

```vue
<template>
  <el-segmented v-model="current" :options="options" block />
</template>

<script setup>
import { ref } from 'vue'
const current = ref('Day')
const options = \['Day', 'Week', 'Month', 'Year']
</script>
```

\---

## 五、Navigation 导航（9个）

### 62\. Affix 固钉

**用途**: 固定元素在可视区域的某个位置。

```vue
<template>
  <el-affix :offset="80">
    <el-button type="primary">固定在距顶部 80px</el-button>
  </el-affix>
</template>
```

|Prop|说明|
|-|-|
|offset|偏移距离|
|position|`top / bottom` 固定方向|
|target|目标元素选择器|
|z-index|z-index|

\---

### 63\. Anchor 锚点（v2.6.0+）

**用途**: 页面内锚点导航。

```vue
<template>
  <el-anchor :offset="20">
    <el-anchor-link href="#basic" title="基础" />
    <el-anchor-link href="#form" title="表单" />
    <el-anchor-link href="#data" title="数据" />
  </el-anchor>
</template>
```

\---

### 64\. Backtop 回到顶部

**用途**: 点击回到页面顶部。

```vue
<template>
  <!-- 向下滚动 200px 后出现 -->
  <el-backtop :visibility-height="200" :right="40" :bottom="40">
    <div style="
        height: 100%;
        width: 100%;
        background-color: #f2f5f6;
        box-shadow: 0 0 6px rgba(0,0,0, .12);
        text-align: center;
        line-height: 40px;
        color: #1989fa;
      ">
      UP
    </div>
  </el-backtop>
</template>
```

\---

### 65\. Breadcrumb 面包屑

**用途**: 显示当前页面路径层级。

```vue
<template>
  <el-breadcrumb separator="/">
    <el-breadcrumb-item :to="{ path: '/' }">首页</el-breadcrumb-item>
    <el-breadcrumb-item><a href="/">活动管理</a></el-breadcrumb-item>
    <el-breadcrumb-item>活动列表</el-breadcrumb-item>
    <el-breadcrumb-item>活动详情</el-breadcrumb-item>
  </el-breadcrumb>

  <!-- 图标分隔符 -->
  <el-breadcrumb :separator-icon="ArrowRight">
    <el-breadcrumb-item :to="{ path: '/' }">首页</el-breadcrumb-item>
    <el-breadcrumb-item>活动详情</el-breadcrumb-item>
  </el-breadcrumb>
</template>
```

\---

### 66\. Dropdown 下拉菜单

**用途**: 触发弹出菜单列表。

```vue
<template>
  <el-dropdown @command="handleCommand">
    <span class="el-dropdown-link">
      更多<el-icon class="el-icon--right"><arrow-down /></el-icon>
    </span>
    <template #dropdown>
      <el-dropdown-menu>
        <el-dropdown-item command="a">黄金糕</el-dropdown-item>
        <el-dropdown-item command="b" divided>狮子头</el-dropdown-item>
        <el-dropdown-item command="c" disabled>螺蛳粉</el-dropdown-item>
        <el-dropdown-item command="d" divided>双皮奶</el-dropdown-item>
      </el-dropdown-menu>
    </template>
  </el-dropdown>
</template>

<script setup>
const handleCommand = cmd => console.log(cmd)
</script>
```

|Trigger|说明|
|-|-|
|trigger|`click / hover / contextmenu` 触发方式|
|split-button|分割按钮模式|
|type|分割按钮类型|
|placement|弹出位置|
|hide-on-click|点击后是否隐藏|
|max-height|菜单最大高度|

\---

### 67\. Menu 菜单

**用途**: 导航菜单（垂直/水平/折叠）。

```vue
<template>
  <el-menu
    :default-active="activeIndex"
    mode="horizontal"
    @select="handleSelect"
  >
    <el-menu-item index="1">Processing Center</el-menu-item>
    <el-sub-menu index="2">
      <template #title>Workspace</template>
      <el-menu-item index="2-1">item one</el-menu-item>
      <el-menu-item index="2-2">item two</el-menu-item>
    </el-sub-menu>
    <el-menu-item index="3" disabled>Info</el-menu-item>
    <el-menu-item index="4">Orders</el-menu-item>
  </el-menu>

  <!-- 侧边栏折叠菜单 -->
  <el-menu
    default-active="2"
    class="el-menu-vertical-demo"
    :collapse="isCollapse"
  >
    <el-menu-item index="2">
      <el-icon><Menu /></el-icon>
      <template #title>Navigator Two</template>
    </el-menu-item>
    <el-sub-menu index="3">
      <template #title>
        <el-icon><Location /></el-icon>
        <span>Navigator Three</span>
      </template>
      <el-menu-item index="3-1">item one</el-menu-item>
      <el-menu-item index="3-2">item two</el-menu-item>
    </el-sub-menu>
  </el-menu>
</template>
```

|Prop|说明|
|-|-|
|mode|`horizontal / vertical` 模式|
|collapse|折叠（仅 vertical）|
|background-color|背景色|
|text-color|文字颜色|
|active-text-color|激活文字色|
|default-active|默认激活|
|router|使用 vue-router|
|unique-opened|同时只展开一个子菜单|
|menu-trigger|`hover / click` 子菜单触发|

\---

### 68\. Page Header 页头

**用途**: 页面标题区域。

```vue
<template>
  <el-page-header @back="goBack" content="详情页面">
    <template #extra>
      <div class="extra-actions">
        <el-button type="primary" text>编辑</el-button>
        <el-button type="primary" text>更多</el-button>
      </div>
    </template>
  </el-page-header>
</template>

<script setup>
const goBack = () => window.history.back()
</script>
```

\---

### 69\. Steps 步骤条

**用途**: 引导用户按步骤完成流程。

```vue
<template>
  <el-steps :active="active" finish-status="success">
    <el-step title="Step 1" description="Some description" />
    <el-step title="Step 2" description="Some description" />
    <el-step title="Step 3" description="Some description" />
  </el-steps>

  <el-button style="margin-top: 12px" @click="next">下一步</el-button>

  <!-- 竖向步骤条 -->
  <el-steps direction="vertical" :active="1">
    <el-step title="Step 1" />
    <el-step title="Step 2" />
    <el-step title="Step 3" />
  </el-steps>
</template>

<script setup>
import { ref } from 'vue'
const active = ref(0)
const next = () => { if (active++ > 2) active = 0 }
</script>
```

|Prop|说明|
|-|-|
|space|每步间距|
|direction|`horizontal / vertical` 方向|
|active|当前激活步骤（从 0 开始）|
|process-status|`wait / process / finish / error / success` 进行中状态|
|finish-status|`wait / process / finish / error / success` 完成状态|
|align-center|居中对齐|
|simple|简洁风格|

\---

### 70\. Tabs 标签页

**用途**: 选项卡切换内容。

```vue
<template>
  <el-tabs v-model="activeName" @tab-click="handleClick" type="border-card">
    <el-tab-pane label="用户管理" name="first">用户管理</el-tab-pane>
    <el-tab-pane label="配置管理" name="second">配置管理</el-tab-pane>
    <el-tab-pane label="角色管理" name="third">角色管理</el-tab-pane>
    <el-tab-pane label="定时任务补偿" name="fourth">定时任务补偿</el-tab-pane>
  </el-tabs>

  <!-- 卡片风格 -->
  <el-tabs v-model="activeTab" type="card" editable @edit="handleTabsEdit">
    <el-tab-pane v-for="(tab, index) in editableTabs" :key="tab.name" :label="tab.title" :name="tab.name">
      {{ tab.content }}
    </el-tab-pane>
  </el-tabs>

  <!-- 位置 -->
  <el-tabs tab-position="left">
    <el-tab-pane label="用户管理">用户管理</el-tab-pane>
    <el-tab-pane label="配置管理">配置管理</el-tab-pane>
  </el-tabs>
</template>
```

|Tabs Prop|说明|
|-|-|
|model-value / v-model|绑定值（激活 tab 的 name）|
|type|`card / border-card` 风格|
|closable|可关闭|
|addable|可新增|
|editable|可编辑|
|tab-position|`top / right / bottom / left` 位置|
|stretch|标签宽度撑满|

\---

## 六、Feedback 反馈组件（10个）

### 71\. Alert 提示

**用途**: 页面内警告/信息提示条。

```vue
<template>
  <el-alert title="成功提示的文案" type="success" effect="dark" />
  <el-alert title="消息提示的文案" type="info" show-icon :closable="false" />
  <el-alert title="警告提示的文案" type="warning" />
  <el-alert title="错误提示的文案" type="error" description="这是一段描述文字" show-icon />
</template>
```

|Prop|说明|
|-|-|
|title|标题|
|type|`success / warning / info / error`|
|description|辅助文字|
|closable|可关闭|
|center|居中|
|show-icon|显示图标|
|effect|`light / dark` 主题|

\---

### 72\. Dialog 对话框

**用途**: 模态弹窗。

```vue
<template>
  <el-button text @click="dialogVisible = true">打开弹窗</el-button>

  <el-dialog
    v-model="dialogVisible"
    title="提示"
    width="30%"
    :before-close="handleClose"
  >
    <span>这是一段信息</span>
    <template #footer>
      <span class="dialog-footer">
        <el-button @click="dialogVisible = false">取消</el-button>
        <el-button type="primary" @click="dialogVisible = false">确定</el-button>
      </span>
    </template>
  </el-dialog>
</template>

<script setup>
import { ref } from 'vue'
const dialogVisible = ref(false)
const handleClose = done => done()
</script>
```

|Prop|说明|
|-|-|
|model-value / v-model|是否显示|
|title|标题|
|width|宽度|
|fullscreen|全屏|
|top|距顶部距离|
|modal|遮罩层|
|lock-scroll|锁定滚动|
|close-on-click-modal|点击遮罩关闭|
|close-on-press-escape|ESC 关闭|
|show-close|显示关闭按钮|
|center|标题/底部居中|
|destroy-on-close|关闭销毁|
|append-to-body|插入 body|
|draggable|可拖拽|
|overflow|内容溢出处理|
|align-center|居中|
|close-icon|自定义关闭图标|

\---

### 73\. Drawer 抽屉

**用途**: 从侧边滑出的抽屉面板。

```vue
<template>
  <el-button type="primary" @click="drawer = true">打开抽屉</el-button>

  <el-drawer
    v-model="drawer"
    title="我是标题"
    direction="rtl"
    :before-close="handleClose"
  >
    <span>我来啦!</span>
  </el-drawer>
</template>

<script setup>
import { ref } from 'vue'
const drawer = ref(false)
const handleClose = done => done()
</script>
```

|Prop|说明|
|-|-|
|model-value / v-model|是否显示|
|direction|`ltr / rtl / ttb / btt` 方向|
|size|大小（数字或百分比）|
|title|标题|
|with-header|是否显示头部|
|modal|遮罩层|
|lock-scroll|锁定滚动|
|close-on-click-modal|点击遮罩关闭|
|append-to-body|插入 body|
|destroy-on-close|关闭销毁|

\---

### 74\. Loading 加载

**用途**: 数据加载中的遮罩指示。

```vue
<template>
  <!-- 指令方式 -->
  <div v-loading="loading" element-loading-text="Loading..." element-loading-background="rgba(122, 122, 122, 0.8)">
    内容...
  </div>

  <!-- 服务方式 -->
  <el-button @click="openLoading">打开全屏 Loading</el-button>
</template>

<script setup>
import { ref } from 'vue'
import { ElLoading } from 'element-plus'

const loading = ref(false)

const openLoading = () => {
  const loadingInstance = ElLoading.service({ fullscreen: true })
  setTimeout(() => loadingInstance.close(), 2000)
}
</script>
```

|v-loading 修饰符|说明|
|-|-|
|v-loading.fullscreen|全屏|
|v-loading.lock|锁屏|
|v-loading.body|插入 body|
|v-loading.custom-class|自定义类名|

|Service Options|说明|
|-|-|
|text|加载文字|
|spinner|自定义 spinner 类名|
|background|背景色|
|target|目标 DOM|
|fullscreen|全屏|
|lock|锁屏|

\---

### 75\. Message 消息提示

**用途**: 轻量级消息反馈（顶部居中弹出）。

```vue
<template>
  <el-button @click="showMsg">显示消息</el-button>
</template>

<script setup>
import { ElMessage } from 'element-plus'

const showMsg = () => {
  ElMessage.success('这是一条成功消息')
  // ElMessage.warning('警告消息')
  // ElMessage.info('信息消息')
  // ElMessage.error('错误消息')

  // 更完整的配置
  ElMessage({
    message: '恭喜你，这是一条成功消息',
    type: 'success',
    duration: 3000,
    showClose: true,
    center: true,
    offset: 100,
    grouping: true,
  })
}
</script>
```

|Option|说明|
|-|-|
|message|消息内容|
|type|`success / warning / info / error`|
|duration|显示时间（ms），0 为不关闭|
|showClose|可关闭|
|center|居中|
|offset|偏移距离|
|grouping|合并相同消息|
|customClass|自定义类名|
|icon|自定义图标|
|dangerouslyUseHTMLString|HTML 内容|

\---

### 76\. Message Box 消息弹出框

**用途**: 模态消息确认框。

```vue
<script setup>
import { ElMessageBox, ElMessage } from 'element-plus'

const open = () => {
  ElMessageBox.alert('这是一段内容', '标题', {
    confirmButtonText: 'OK',
    callback: action => {
      ElMessage.info(`action: ${action}`)
    },
  })
}

const confirm = () => {
  ElMessageBox.confirm('确定要执行此操作吗?', '提示', {
    confirmButtonText: '确定',
    cancelButtonText: '取消',
    type: 'warning',
  }).then(() => {
    ElMessage.success('删除成功!')
  }).catch(() => {
    ElMessage.info('已取消删除')
  })
}

const prompt = () => {
  ElMessageBox.prompt('请输入邮箱', '提示', {
    confirmButtonText: '确定',
    cancelButtonText: '取消',
    inputPattern: /\[\\w!#$%\&'\*+/=?^\_`{|}\~-]+(?:\\.\[\\w!#$%\&'\*+/=?^\_`{|}\~-]+)\*@(?:\[\\w](?:\[\\w-]\*\[\\w])?\\.)+\[\\w](?:\[\\w-]\*\[\\w])?/,
    inputErrorMessage: '邮箱格式不正确',
  }).then(({ value }) => {
    ElMessage.success(`你的邮箱是: ${value}`)
  }).catch(() => {})
}
</script>
```

|方法|说明|
|-|-|
|ElMessageBox.alert(message, title, options?)|警告框|
|ElMessageBox.confirm(message, title, options?)|确认框|
|ElMessageBox.prompt(message, title, options?)|输入框|
|ElMessageBox.msgbox(options?)|通用|

\---

### 77\. Notification 通知

**用途**: 右上角全局通知提醒。

```vue
<script setup>
import { ElNotification } from 'element-plus'

const notify = () => {
  ElNotification({
    title: '标题名称',
    message: '这是提示文案这是提示文案这是提示文案这是提示文案这是提示文案',
    type: 'success',
    position: 'top-right', // top-right / top-left / bottom-right / bottom-left
    duration: 4500,
    showClose: true,
  })
}
</script>
```

|Option|说明|
|-|-|
|title|标题|
|message|内容|
|type|`success / warning / info / error`|
|position|位置|
|duration|显示时间|
|showClose|可关闭|
|onClick|点击回调|
|onClose|关闭回调|
|icon|自定义图标|
|dangerouslyUseHTMLString|HTML 内容|

\---

### 78\. Popconfirm 气泡确认框

**用途**: 点击目标后气泡式二次确认。

```vue
<template>
  <el-popconfirm title="确定删除吗?" @confirm="handleConfirm" @cancel="handleCancel">
    <template #reference>
      <el-button>删除</el-button>
    </template>
  </el-popconfirm>
</template>

<script setup>
const handleConfirm = () => console.log('确认了')
const handleCancel = () => console.log('取消了')
</script>
```

|Prop|说明|
|-|-|
|title|标题|
|confirm-button-text|确认按钮文字|
|cancel-button-text|取消按钮文字|
|confirm-button-type|确认按钮类型|
|icon|图标|
|icon-color|图标颜色|
|width|宽度|
|hide-after|自动关闭延迟|
|teleported|是否插入 body|

\---

### 79\. Popover 弹出框

**用途**: 点击/悬停触发的弹出内容面板。

```vue
<template>
  <el-popover
    placement="top-start"
    :width="200"
    trigger="hover"
    content="this is content, this is content, this is content"
  >
    <template #reference>
      <el-hover-container>Hover to activate</el-hover-container>
    </template>
  </el-popover>

  <!-- 嵌套信息 -->
  <el-popover trigger="focus" placement="top" width="auto">
    <template #reference>
      <el-input v-model="input" placeholder="聚焦时显示" />
    </template>
    <div>这是一段内容</div>
  </el-popover>
</template>
```

|Prop|说明|
|-|-|
|trigger|`click / focus / hover / contextmenu` 触发方式|
|title|标题|
|width|宽度|
|placement|出现位置|
|content|内容（字符串）|
|disabled|禁用|
|visible / v-model|是否显示|
|transition|过渡动画|
|popper-class|自定义类名|
|popper-style|自定义样式|
|offset|偏移量|
|show-after / hide-after|显示/隐藏延迟|
|teleported|是否插入 body|

\---

### 80\. Tooltip 文字提示

**用途**: 悬停显示文字说明。

```vue
<template>
  <el-tooltip content="Top center" placement="top">
    <el-button>Dark</el-button>
  </el-tooltip>

  <el-tooltip content="Bottom center" placement="bottom" effect="light">
    <el-button>Light</el-button>
  </el-tooltip>

  <el-tooltip :disabled="disabled">
    <template #content>
      多行信息<br/>第二行信息
    </template>
    <el-button>{{ disabled ? '已禁用' : '悬停查看' }}</el-button>
  </el-tooltip>
</template>
```

|Prop|说明|
|-|-|
|content|内容|
|placement|12 个方向：`top / top-start / top-end / bottom / ...`|
|effect|`dark / light` 主题|
|disabled|禁用|
|offset|偏移量|
|transition|过渡动画|
|show-after / hide-after|显示/隐藏延迟(ms)|
|popper-class|自定义类名|
|virtual-triggering|虚拟触发|
|virtual-ref|虚拟触发引用|

\---

## 七、Others 其他（2个）

### 81\. Divider 分割线

**用途**: 内容分割线。

```vue
<template>
  <el-divider />
  <el-divider content-position="left">左侧文字</el-divider>
  <el-divider content-position="center">居中文字</el-divider>
  <el-divider content-position="right">右侧文字</el-divider>
  <el-divider border-style="dashed" />
  <el-divider :orientation="'vertical'" />
</template>
```

|Prop|说明|
|-|-|
|direction|`horizontal / vertical` 方向|
|content-position|`left / center / right` 文字位置|
|border-style|`solid / dashed / dotted` 线型|

\---

### 82\. Watermark 水印（v2.4.0+）

**用途**: 页面水印（防截图）。

```vue
<template>
  <el-watermark
    :content="\['Element Plus', 'Watermark']"
    :font="{ color: 'rgba(0, 0, 0, .15)' }"
  >
    <div style="height: 500px;">
      这里的内容会带有水印效果
    </div>
  </el-watermark>

  <!-- 图片水印 -->
  <el-watermark :image="imageSrc">
    <div style="height: 500px;">
      图片水印内容
    </div>
  </el-watermark>
</template>
```

|Prop|说明|
|-|-|
|content|水印文字（string 或 string\[]）|
|image|水印图片 URL|
|font|字体配置（color/fontFamily/fontSize/fontWeight/rotate 等）|
|width / height|水印宽高|
|rotate|旋转角度|
|zIndex|z-index|
|gap|水印间距|
|offset|偏移量|

\---

## 附录：通用约定与最佳实践

### 安装与引入

```bash
# 安装
npm install element-plus
npm install @element-plus/icons-vue   # 图标库

# 完整引入（不推荐生产环境）
import ElementPlus from 'element-plus'
import 'element-plus/dist/index.css'
app.use(ElementPlus)

# 按需引入（推荐）
// 使用 unplugin-vue-components 和 unplugin-auto-import
// vite.config.ts:
import AutoImport from 'unplugin-auto-import/vite'
import Components from 'unplugin-vue-components/vite'
import { ElementPlusResolver } from 'unplugin-vue-components/resolvers'

export default defineConfig({
  plugins: \[
    AutoImport({ resolvers: \[ElementPlusResolver()] }),
    Components({ resolvers: \[ElementPlusResolver()] }),
  ],
})
```

### 常用全局 API

```javascript
import { ElMessage, ElMessageBox, ElNotification, ElLoading } from 'element-plus'

// 消息提示
ElMessage.success('成功')
ElMessage.error('错误')
ElMessage.warning('警告')
ElMessage.info('信息')

// 确认框
await ElMessageBox.confirm('确定吗？', '提示')

// 通知
ElNotification.success({ title: '成功', message: '操作成功' })

// 全屏加载
const instance = ElLoading.service({ fullscreen: true })
instance.close()
```

### 常用 CSS 变量

```css
/\* Element Plus 内置 CSS 变量 \*/
:root {
  /\* 品牌色 \*/
  --el-color-primary: #409EFF;
  --el-color-primary-light-3: #79bbff;
  --el-color-primary-light-5: #a0cfff;
  --el-color-primary-light-7: #c6e2ff;
  --el-color-primary-light-9: #ecf5ff;
  --el-color-dark-2: #337ecc;

  /\* 功能色 \*/
  --el-color-success: #67C23A;
  --el-color-warning: #E6A23C;
  --el-color-danger: #F56C6C;
  --el-color-info: #909399;

  /\* 边框 \*/
  --el-border-color: #dcdfe6;
  --el-border-radius-base: 4px;

  /\* 字体 \*/
  --el-font-size-base: 14px;

  /\* 背景 \*/
  --el-bg-color: #ffffff;
  --el-bg-color-overlay: #ffffff;
}
```

### 组件命名规范

|命名方式|示例|说明|
|-|-|-|
|kebab-case 模板|`<el-button>`|Vue 模板中使用|
|PascalCase 引入|`import { ElButton } from 'element-plus'`|JS 中导入|
|camelCase 事件|`@change`, `@size-change`|事件监听|
|kebab-case Props|`:show-password`, `:allow-create`|属性传递|

### 版本兼容性

* **最低要求**: Node.js >= 12.22.0
* **框架**: Vue >= 3.2.0
* **打包工具**: Vite / Webpack 5+
* **浏览器**: Chrome >= 87, Firefox >= 78, Safari >= 14, Edge >= 88

### 快速查找指南

|需求|推荐组件|
|-|-|
|用户输入文本|Input, Textarea, InputNumber, InputTag|
|用户做选择|Select, Checkbox, Radio, Switch, Slider, Cascader, Transfer|
|日期/时间|DatePicker, TimePicker, DateTimePicker|
|文件操作|Upload|
|数据展示|Table, Descriptions, Card, Tag, Statistic|
|导航路由|Menu, Tabs, Breadcrumb, Steps, PageHeader|
|反馈提示|Message, Notification, Dialog, Drawer, Alert, Loading|
|布局排版|Container, Layout, Row/Col, Space, Splitter|
|图标|Icon (@element-plus/icons-vue)|
|大数据优化|VirtualizedSelect, VirtualizedTable, VirtualizedTree|
|新手引导|Tour|
|安全防护|Watermark|

\---

> 📖 \*\*本文档基于 Element Plus v2.14.1\*\*  
> 🔗 完整 API 文档: https://cn.element-plus.org/zh-CN/component/  
> 💻 GitHub: https://github.com/element-plus/element-plus

