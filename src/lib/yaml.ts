/**
 * 简易 YAML 工具：仅处理扁平 key: value 形式，用于设置项读写。
 * 不支持嵌套对象、数组、多行字符串等复杂结构。
 * 注释行（# 开头）和空行会被保留但跳过解析。
 */

/** 从 YAML 文本中读取指定 key 的字符串值 */
export function yamlGet(yaml: string, key: string): string | undefined {
  const lines = yaml.split("\n");
  const prefix = `${key}:`;
  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed.startsWith(prefix)) {
      const rest = trimmed.slice(prefix.length).trim();
      // 去掉行内注释
      const commentIdx = rest.indexOf(" #");
      const value = commentIdx >= 0 ? rest.slice(0, commentIdx).trim() : rest;
      // 去掉引号
      if (
        (value.startsWith('"') && value.endsWith('"')) ||
        (value.startsWith("'") && value.endsWith("'"))
      ) {
        return value.slice(1, -1);
      }
      return value;
    }
  }
  return undefined;
}

/** 设置 YAML 文本中指定 key 的值，若不存在则追加 */
export function yamlSet(yaml: string, key: string, value: string): string {
  const lines = yaml.split("\n");
  const prefix = `${key}:`;
  let found = false;

  for (let i = 0; i < lines.length; i++) {
    const trimmed = lines[i].trim();
    if (trimmed.startsWith(prefix)) {
      // 保留原有缩进
      const indent = lines[i].slice(0, lines[i].length - trimmed.length);
      lines[i] = `${indent}${prefix} ${formatValue(value)}`;
      found = true;
      break;
    }
  }

  if (!found) {
    lines.push(`${prefix} ${formatValue(value)}`);
  }

  return lines.join("\n");
}

function formatValue(value: string): string {
  if (value === "") return '""';
  // 包含特殊字符则加引号
  if (/[:#\-?,&*!|>'"%@`]/.test(value) || /\s/.test(value)) {
    return `"${value.replace(/"/g, '\\"')}"`;
  }
  return value;
}
