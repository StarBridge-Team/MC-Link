import { ref } from "vue";
import { checkUpdate, downloadUpdate, clearUpdateCache } from "../lib/api/update";
import type { DownloadUpdateResult, UpdateInfo } from "../lib/api/types";

export function useUpdater() {
  const checking = ref(false);
  const downloading = ref(false);
  const hasUpdate = ref(false);
  const currentVersion = ref("");
  const updateInfo = ref<UpdateInfo | null>(null);
  const downloadResult = ref<DownloadUpdateResult | null>(null);
  const error = ref<string | null>(null);

  async function check() {
    checking.value = true;
    error.value = null;
    try {
      const result = await checkUpdate();
      hasUpdate.value = result.has_update;
      currentVersion.value = result.current_version;
      updateInfo.value = result.latest ?? null;
      downloadResult.value = null;
      return result;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      checking.value = false;
    }
  }

  async function download(info?: UpdateInfo) {
    const target = info ?? updateInfo.value;
    if (!target) {
      const msg = "没有可用的更新信息";
      error.value = msg;
      throw new Error(msg);
    }

    downloading.value = true;
    error.value = null;
    try {
      const result = await downloadUpdate(target);
      downloadResult.value = result;
      return result;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      downloading.value = false;
    }
  }

  async function clearCache() {
    await clearUpdateCache();
    downloadResult.value = null;
  }

  return {
    checking,
    downloading,
    hasUpdate,
    currentVersion,
    updateInfo,
    downloadResult,
    error,
    check,
    download,
    clearCache,
  };
}
