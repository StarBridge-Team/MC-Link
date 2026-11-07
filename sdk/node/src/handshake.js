// 握手：明文 JSON 的三次往返 + 密钥派生。
//
// 单独成文件的原因：它是协议里**最容易写错**的一段（转录串 role、PSK 解码、
// salt/info 拼法），和"连接后的消息分发"混在一起时，改一处很容易碰到另一处。

import { fromHex, randomHex, utf8 } from "./bytes.js";
import { hkdfSha256, hmacSha256Hex } from "./crypto.js";
import { INFO_C2S, INFO_S2C, PROTOCOL_VERSION, transcript } from "./protocol.js";

/** 握手等待上限（毫秒）。略短于核心的 15 秒，避免两边同时判超时。 */
export const HANDSHAKE_TIMEOUT_MS = 14_000;

/** 带错误码的握手失败。 */
export class HandshakeError extends Error {
  constructor(code, message) {
    super(message);
    this.name = "HandshakeError";
    this.code = code;
  }
}

/**
 * 握手阶段的明文帧队列。
 *
 * 握手前消息只能是明文，握手后是加密信封；用这个队列把"提前到达的帧"接住，
 * 等 `ready` 之后再由主循环补处理，避免丢帧。
 */
export function createPreQueue() {
  const items = [];
  const waiters = [];
  return {
    push(raw) {
      const waiter = waiters.shift();
      if (waiter) waiter(raw);
      else items.push(raw);
    },
    /** 取下一帧；超时抛 `timeout`。 */
    next(message, timeoutMs = HANDSHAKE_TIMEOUT_MS) {
      if (items.length > 0) return Promise.resolve(items.shift());
      return new Promise((resolve, reject) => {
        const timer = setTimeout(
          () => reject(new HandshakeError("timeout", message)),
          timeoutMs,
        );
        waiters.push((raw) => {
          clearTimeout(timer);
          resolve(raw);
        });
      });
    },
    /** 已经提前到达、尚未消费的帧（交给主循环）。 */
    drain() {
      return items.splice(0, items.length);
    },
  };
}

/**
 * 完成握手，返回会话密钥与元数据。
 *
 * @param {WebSocket} ws 已 open 的连接
 * @param {{ plugin_id: string, psk: string }} session 会话文件内容
 * @param {ReturnType<typeof createPreQueue>} pre 明文帧队列
 * @param {string} pluginVersion `hello_ack` 里自报的版本
 */
export async function performHandshake(ws, session, pre, pluginVersion) {
  const hello = parsePlain(await pre.next("等待 hello 超时"));
  if (hello.t !== "hello") {
    throw new HandshakeError("unauthorized", `期望 hello，收到 ${hello.t}`);
  }
  if (hello.v !== PROTOCOL_VERSION) {
    throw new HandshakeError("unauthorized", `协议版本不匹配：${hello.v}`);
  }
  if (hello.expect_plugin !== session.plugin_id) {
    throw new HandshakeError("unauthorized", "核心期望的插件 ID 与会话文件不一致");
  }

  const sid = hello.session;
  const nonceS = hello.nonce_s;
  const nonceP = randomHex(32);
  // PSK 在会话文件里是 hex 文本，必须先解码成 32 字节再当 HMAC 的 key
  const psk = fromHex(session.psk);

  const proof = await hmacSha256Hex(
    psk,
    utf8(transcript(sid, session.plugin_id, nonceS, nonceP, "plugin")),
  );
  ws.send(
    JSON.stringify({
      t: "hello_ack",
      plugin_id: session.plugin_id,
      plugin_version: pluginVersion,
      nonce_p: nonceP,
      proof,
    }),
  );

  const auth = parsePlain(await pre.next("等待 auth_ok 超时"));
  if (auth.t === "denied") {
    throw new HandshakeError(auth.code ?? "unauthorized", auth.message ?? "核心拒绝认证");
  }
  if (auth.t !== "auth_ok") {
    throw new HandshakeError("unauthorized", `期望 auth_ok，收到 ${auth.t}`);
  }

  const expectCore = await hmacSha256Hex(
    psk,
    utf8(transcript(sid, session.plugin_id, nonceS, nonceP, "core")),
  );
  if (expectCore !== auth.proof) {
    throw new HandshakeError("unauthorized", "核心证明无效（可能连到了假核心）");
  }

  // salt = nonce_s || nonce_p（两个 hex 字符串的 ASCII 拼接）；
  // info = 方向前缀 **拼接** role="session" 的转录串
  const salt = utf8(nonceS + nonceP);
  const tSession = transcript(sid, session.plugin_id, nonceS, nonceP, "session");
  const c2s = await hkdfSha256(psk, salt, [INFO_C2S, tSession]);
  const s2c = await hkdfSha256(psk, salt, [INFO_S2C, tSession]);

  return { sid, keys: { c2s, s2c }, heartbeatMs: auth.heartbeat_ms ?? 5000 };
}

function parsePlain(raw) {
  try {
    return JSON.parse(String(raw));
  } catch {
    throw new HandshakeError("unauthorized", "握手报文不是合法 JSON");
  }
}

/** 等 WebSocket 进入 open（`onopen` 在两种运行时都可用）。 */
export function waitOpen(ws, timeoutMs = HANDSHAKE_TIMEOUT_MS) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new HandshakeError("timeout", "WebSocket 建连超时")),
      timeoutMs,
    );
    ws.onopen = () => {
      clearTimeout(timer);
      resolve();
    };
  });
}
