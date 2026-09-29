import { ElMessage } from "element-plus";

export function useToast() {
  function showToast(msg: string, isError?: boolean) {
    ElMessage({
      message: msg,
      type: (isError ?? false) ? "error" : "success",
      duration: 2000,
      grouping: true,
      placement: "bottom-left",
      offset: 16,
    });
  }

  function clearAllTimers() {}

  return { showToast, clearAllTimers };
}
