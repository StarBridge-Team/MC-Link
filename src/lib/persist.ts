/**
 * 统一本地存储层。
 *
 * 所有 `localStorage` / `sessionStorage` 访问都必须经由此处（`scripts/check-arch.mjs`
 * 会拦直接访问）：
 *
 * 1. **key 集中登记**（见 `KEYS`），避免同一份数据在多个文件里用不同拼写读写；
 * 2. **失败不再静默**：配额不足 / 隐私模式下的写失败会 `console.warn`，
 *    而不是用空 `catch` 吞掉，导致"改了却没保存"无从察觉；
 * 3. 将来若要改为存到后端统一持久化，只需改这一个文件。
 *
 * 注意：`KEYS` 里的字符串**必须与历史版本保持一致**。改名会让老用户的
 * localStorage 数据变成孤儿（表现为"设置丢失"），如确需改名，必须先写迁移。
 */

/** 全部受管的存储键。 */
export const KEYS = {
  /** 窗口位置与大小（含 x/y/width/height 的 JSON） */
  windowState: "window_state",
  /** M3 主题参数（seed/variant/contrast 的 JSON） */
  m3Theme: "m3-theme-config",
  /** 玩家名 */
  playerName: "player_name",
  /** 快速联机表单（联机能力重接后使用） */
  flashCode: "flash_code",
  flashMode: "flash_mode",
  /** 高级联机表单（前缀 + code/mode/signaling/stun/nat_stun/app_type） */
  advancedPrefix: "gaoji_p2p_",
  /** 联机页当前标签 */
  activeTab: "active_tab",
  /** 设置页当前二级标签 */
  settingTab: "setting_tab",
  /** 插件管理界面的筛选条件 */
  pluginFilter: "plugin_filter",
} as const;

function warn(action: string, key: string, err: unknown): void {
  console.warn(`[persist] ${action} 失败（${key}）:`, err);
}

function makeStore(storage: () => Storage) {
  return {
    /** 读取并 JSON 解析；失败或不存在时返回 `fallback`。 */
    get<T>(key: string, fallback: T): T {
      try {
        const raw = storage().getItem(key);
        if (raw === null) return fallback;
        return JSON.parse(raw) as T;
      } catch (e) {
        warn("读取", key, e);
        return fallback;
      }
    },

    /** JSON 序列化写入；返回是否成功。 */
    set(key: string, value: unknown): boolean {
      try {
        storage().setItem(key, JSON.stringify(value));
        return true;
      } catch (e) {
        warn("写入", key, e);
        return false;
      }
    },

    /** 读取原始字符串。 */
    getString(key: string, fallback = ""): string {
      try {
        return storage().getItem(key) ?? fallback;
      } catch (e) {
        warn("读取", key, e);
        return fallback;
      }
    },

    /** 写入原始字符串；返回是否成功。 */
    setString(key: string, value: string): boolean {
      try {
        storage().setItem(key, value);
        return true;
      } catch (e) {
        warn("写入", key, e);
        return false;
      }
    },

    remove(key: string): void {
      try {
        storage().removeItem(key);
      } catch (e) {
        warn("删除", key, e);
      }
    },
  };
}

/** 长期存储（localStorage）。 */
export const local = makeStore(() => localStorage);

/** 会话级存储（sessionStorage，页面关闭即清）。 */
export const session = makeStore(() => sessionStorage);
