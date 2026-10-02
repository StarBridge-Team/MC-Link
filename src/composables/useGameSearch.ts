import { reactive } from "vue";
import type { GameTab } from "../router";

/**
 * 「游戏」页每个模式各自的搜索词。
 *
 * 模块级单例（与本项目其它 composable 一致）。工具栏是**唯一的**搜索输入框，切换 Tab 时
 * 它的位置与外观都不变，变的只是绑定的那个词 —— 每个模式各记各的，切回来还在。
 *
 * 这里只存词、不存结果：怎么用由各模式自己决定，因为合适的做法并不相同：
 * - 「插件」把词交给后端 `plugin_list` 统一过滤（facets 计数才不会与列表分叉）；
 * - 「游戏」条目只有几十条，前端本地过滤更省一次 IPC；
 * - 「市场」暂时没有数据源，词先存着。
 */
const queries = reactive<Record<GameTab, string>>({
  market: "",
  games: "",
  plugins: "",
});

export function useGameSearch() {
  return { queries };
}
