import { ref } from "vue";
import { getPageManifest, getPageContent, clearPageCache } from "../lib/api/page";
import type { PageManifest, PageEntry } from "../lib/api/types";

export function usePageLoader() {
  const loading = ref(false);
  const error = ref<string | null>(null);
  const manifest = ref<PageManifest | null>(null);
  const content = ref<string>("");

  async function loadManifest() {
    loading.value = true;
    error.value = null;
    try {
      manifest.value = await getPageManifest();
      return manifest.value;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      loading.value = false;
    }
  }

  async function loadPage(name: string) {
    loading.value = true;
    error.value = null;
    try {
      content.value = await getPageContent(name);
      return content.value;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      loading.value = false;
    }
  }

  async function refresh() {
    await clearPageCache();
    return loadManifest();
  }

  function findPage(name: string): PageEntry | undefined {
    return manifest.value?.pages.find((p) => p.name === name);
  }

  return {
    loading,
    error,
    manifest,
    content,
    loadManifest,
    loadPage,
    refresh,
    findPage,
  };
}
