/**
 * `@m3e/web` 的组件注册。
 *
 * 抽成一个文件是因为现在有多个 HTML 入口（主窗口 / 房间窗口 / 小组件），
 * 每个入口都抄一遍这串 import 必然会漂移——某天新增一个组件只在一个入口注册，
 * 另一个窗口里就变成"标签存在但什么都不显示"。
 *
 * 按需注册：每个包注册一批自定义元素，不要改成整包引入。
 */
import "@m3e/web/theme"; // <m3e-theme>（动态配色 + 动效方案）
import "@m3e/web/icon"; // <m3e-icon>
import "@m3e/web/button"; // <m3e-button>
import "@m3e/web/icon-button"; // <m3e-icon-button>（左下角快捷入口）
import "@m3e/web/card"; // <m3e-card>
import "@m3e/web/switch"; // <m3e-switch>
import "@m3e/web/checkbox"; // <m3e-checkbox>
import "@m3e/web/slider"; // <m3e-slider>/<m3e-slider-thumb>
import "@m3e/web/select"; // <m3e-select>/<m3e-option>
import "@m3e/web/option"; // <m3e-option>
import "@m3e/web/form-field"; // <m3e-form-field>
import "@m3e/web/search"; // <m3e-search-bar>
import "@m3e/web/avatar"; // <m3e-avatar>
import "@m3e/web/progress-indicator"; // <m3e-linear-progress-indicator>
import "@m3e/web/nav-rail"; // 左侧主导航
import "@m3e/web/nav-bar"; // <m3e-nav-bar>/<m3e-nav-item>
import "@m3e/web/nav-menu"; // 设置二级导航
import "@m3e/web/app-bar"; // 标题栏
import "@m3e/web/badge"; // <m3e-badge>
import "@m3e/web/dialog"; // <m3e-dialog>
import "@m3e/web/loading-indicator"; // <m3e-loading-indicator>
