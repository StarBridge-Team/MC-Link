/**
 * 极简 YAML「扁平键值」读写。
 *
 * # 为什么需要它
 *
 * `Setting/*.yml` 里除了个性化设置，还有几个后端**没有结构定义**的分区
 * （如 `account.yml`、`connector.yml`）——后端只提供 `get_setting` /
 * `save_setting` 两个"整段原文"通道，内容契约完全在前端。
 *
 * # 为什么不引 YAML 库
 *
 * 这些文件是扁平的 `key: value`，引一个完整 YAML 实现只为读三个字段，既增加
 * 依赖也增加了"重新序列化后把用户手写的注释、顺序、格式全部丢掉"的风险。
 * 因此这里**只做行级改写**：认识的键改值、不认识的键与注释原样保留。
 * 代价是不支持嵌套结构——一旦某个分区需要嵌套，就该由后端给出结构定义。
 */

/** 扁平键值对。 */
export type FlatYaml = Record<string, string>;

const LINE = /^(\s*)([A-Za-z0-9_.-]+)\s*:\s*(.*)$/;

/** 解析顶层 `key: value`；注释、空行与嵌套结构一律跳过。 */
export function parseFlatYaml(text: string): FlatYaml {
  const out: FlatYaml = {};
  for (const raw of text.split(/\r?\n/)) {
    if (!raw.trim() || raw.trimStart().startsWith("#")) continue;
    const match = LINE.exec(raw);
    if (!match) continue;
    const [, indent, key, value] = match;
    // 有缩进说明它是嵌套在某个键下面的，不属于顶层。
    if (indent.length > 0) continue;
    out[key] = unquote(value);
  }
  return out;
}

/**
 * 把 `patch` 里的键写回 YAML 原文，**保持其余内容逐字不变**。
 *
 * 已存在的键就地改值；不存在的键追加到末尾。返回值可直接交给 `save_setting`。
 */
export function writeFlatYaml(text: string, patch: FlatYaml): string {
  const remaining = new Map(Object.entries(patch));
  const lines = text.length > 0 ? text.split(/\r?\n/) : [];

  const updated = lines.map((raw) => {
    if (!raw.trim() || raw.trimStart().startsWith("#")) return raw;
    const match = LINE.exec(raw);
    if (!match) return raw;
    const [, indent, key] = match;
    if (indent.length > 0 || !remaining.has(key)) return raw;
    const value = remaining.get(key) ?? "";
    remaining.delete(key);
    return `${indent}${key}: ${formatValue(value)}`;
  });

  for (const [key, value] of remaining) {
    updated.push(`${key}: ${formatValue(value)}`);
  }

  // 末尾保证恰好一个换行，避免每保存一次就多出空行。
  return `${updated.join("\n").replace(/\n+$/, "")}\n`;
}

function unquote(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length >= 2 && /^(".*"|'.*')$/.test(trimmed)) {
    return trimmed.slice(1, -1);
  }
  // 去掉行尾注释（`key: value # 说明`）；只在明显是注释时才切。
  const hash = trimmed.indexOf(" #");
  return hash > 0 ? trimmed.slice(0, hash).trim() : trimmed;
}

function formatValue(value: string): string {
  const trimmed = value.trim();
  if (trimmed === "") return "";
  // 需要引号的情况：含 YAML 特殊字符、被空格包裹、或看起来像数字/布尔但其实是字符串。
  const needsQuote = /[:#{}[\],&*?|>!%@`"']/.test(trimmed) || trimmed !== value;
  if (!needsQuote) return trimmed;
  return `"${trimmed.replace(/"/g, '\\"')}"`;
}
