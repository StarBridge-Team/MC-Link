/**
 * 地区访问限制的**唯一状态源**。
 *
 * 判定只有后端能做（要查公网 IP 归属地，见 `src-tauri/src/commands/region.rs`），
 * 这里只负责"查一次、记住、给所有界面用"：
 *
 * - 为什么要缓存：主窗口里导航栏、联机页、主页入口都要读它，各自发一次 IPC 既慢
 *   又可能出现"导航栏已隐藏、页面还没拦"的中间状态；
 * - 为什么只在启动查一次：归属地不会在一次运行期间变化，频繁重查只是多打服务。
 *   换网络（比如挂上代理）要重启应用才生效，这与"换壁纸要重启"是同一类取舍。
 *
 * # 失败一律按"不受限"处理
 *
 * 需求是"其他地区用户不受影响"。判定失败时按受限处理会让所有人在网络抖动时
 * 一起失去功能，那是更糟的事故；漏拦一次则会在服务恢复后自愈。
 */
import { computed, ref } from "vue";
import { defineStore } from "pinia";

import { getRegionInfo } from "../lib/api/region";

/** 并发调用 `ensure()` 时共用同一次请求。 */
let inflight: Promise<void> | null = null;

export const useRegionStore = defineStore("region", () => {
  /** 是否已经查过（成功或失败都算，避免失败后无限重试）。 */
  const checked = ref(false);
  const loading = ref(false);
  const mainland = ref(false);
  /** 原始地区串，只用于排查误判。 */
  const regionText = ref("");
  const error = ref<string | null>(null);

  /** 是否拦掉联机相关界面。**未查完不算拦**——不能让界面先闪一下提示页。 */
  const blocked = computed(() => checked.value && mainland.value);

  async function ensure(): Promise<void> {
    if (checked.value) return;
    if (inflight) return inflight;

    loading.value = true;
    inflight = (async () => {
      try {
        const info = await getRegionInfo();
        mainland.value = info.mainland_china;
        regionText.value = info.region;
        error.value = null;
        if (info.mainland_china) {
          console.info(`[region] 当前地区判定为受限（${info.region}）`);
        }
      } catch (e) {
        mainland.value = false;
        regionText.value = "";
        error.value = e instanceof Error ? e.message : String(e);
        console.warn("[region] 地区判定失败，按不受限处理:", e);
      } finally {
        loading.value = false;
        checked.value = true;
        inflight = null;
      }
    })();
    return inflight;
  }

  return { checked, loading, mainland, regionText, error, blocked, ensure };
});
