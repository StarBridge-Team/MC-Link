/**
 * 邀请链接的解析与构造。
 *
 * 两个方向必须严格对称：`extractInviteCode` 解出来的码，`buildShareLink` 拼回去
 * 必须还能解出同一个值。房间码可能带 `/`（陶瓦会给 `U/A90T-7XHQ-9T98-ECQV`），
 * 所以两边都走 URL 编解码。
 */

/** 从深链 / 查询串 / 纯文本里抽出房间码；抽不到返回空串。 */
export function extractInviteCode(text: string): string {
  const t = text.trim();

  // `mclink://join/<code>`：不能用 `[^/?#\s]+`，房间码自身可能含 `/`。
  // 取 `join/` 之后直到结尾或空白，因为码是路径最后一段。
  const deep = t.match(/mclink:\/\/join\/([^\s#]+)/i);
  if (deep) return safeDecode(deep[1]);

  // `...?code=<code>`：这里才需要在意 `&`。
  const query = t.match(/[?&]code=([^&\s#]+)/i);
  if (query) return safeDecode(query[1]);

  return t;
}

/** 构造分享链接。 */
export function buildShareLink(roomCode: string): string {
  return `mclink://join/${encodeURIComponent(roomCode)}`;
}

function safeDecode(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}
