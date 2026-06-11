/**
 * SessionStorage 集中管理模块
 * 用于存储无需长期保留的临时数据（输入框内容、临时状态等）
 * 页面关闭后数据自动清除
 */

const PREFIX = 'mc_session_';

function get<T>(key: string, fallback: T): T {
  try {
    const raw = sessionStorage.getItem(PREFIX + key);
    if (raw === null) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

function set(key: string, value: unknown): void {
  try {
    sessionStorage.setItem(PREFIX + key, JSON.stringify(value));
  } catch {
    // sessionStorage 不可用时静默失败
  }
}

function strGet(key: string, fallback = ''): string {
  try {
    return sessionStorage.getItem(PREFIX + key) ?? fallback;
  } catch {
    return fallback;
  }
}

function strSet(key: string, value: string): void {
  try {
    sessionStorage.setItem(PREFIX + key, value);
  } catch {
    // ignore
  }
}

function remove(key: string): void {
  try {
    sessionStorage.removeItem(PREFIX + key);
  } catch {
    // ignore
  }
}

function clear(): void {
  try {
    const keys: string[] = [];
    for (let i = 0; i < sessionStorage.length; i++) {
      const k = sessionStorage.key(i);
      if (k && k.startsWith(PREFIX)) {
        keys.push(k);
      }
    }
    keys.forEach(k => sessionStorage.removeItem(k));
  } catch {
    // ignore
  }
}

export const session = {
  get,
  set,
  strGet,
  strSet,
  remove,
  clear,
};