// 协议常量与报文构造：与 `src-tauri/src/plugin/protocol.rs` 一一对应。
//
// 任何一处拼写不一致都会表现为"连上了但行为不对"，因此这里逐条照抄常量，
// 不做"顺手改个名更好看"的本地优化。

/** 协议版本。不匹配时核心直接拒绝连接。 */
export const PROTOCOL_VERSION = 1;

/** WebSocket 子协议名。 */
export const WS_SUBPROTOCOL = "mclink-plugin";

/** 握手 transcript 前缀。 */
export const AUTH_PREFIX = "mclink-plugin-auth-v1";

/** 派生密钥用的 HKDF info 前缀。 */
export const INFO_C2S = "mclink-plugin-c2s-v1";
export const INFO_S2C = "mclink-plugin-s2c-v1";

/** 核心 → 插件的方法名。 */
export const CoreMethod = Object.freeze({
  PING: "core.ping",
  INFO: "core.info",
  LOG: "core.log",
  CONTEXT_UPDATE: "core.context.update",
  SHUTDOWN: "core.shutdown",
  PEER_LIST: "core.peer.list",
  PEER_CALL: "core.peer.call",
  PEER_PUBLISH: "core.peer.publish",
});

/** 插件 → 核心：peer 信道固定的线上名字。 */
export const PEER_CALL = "peer.call";
export const PEER_EVENT = "peer.event";

/** 约定的事件主题（可自定义，这些只是与核心/界面约定好的那几个）。 */
export const EventTopic = Object.freeze({
  ADAPTER_STATE: "adapter.state",
  DETECTOR_FOUND: "detector.found",
  DETECTOR_LOST: "detector.lost",
  COUPLER_STATE: "coupler.state",
  BRIDGE_STATE: "bridge.state",
  WIDGET_STATE: "widget.state",
  UTILITY_STATE: "utility.state",
  LOG: "plugin.log",
});

/** 错误码（与核心 `error_code` 模块同值）。 */
export const ErrorCode = Object.freeze({
  BAD_REQUEST: "bad_request",
  UNAUTHORIZED: "unauthorized",
  FORBIDDEN: "forbidden",
  NOT_SUPPORTED: "not_supported",
  TIMEOUT: "timeout",
  RATE_LIMITED: "rate_limited",
  INTERNAL: "internal",
  UNAVAILABLE: "unavailable",
});

/** 握手阶段双方必须完全一致的转录串。 */
export function transcript(session, pluginId, nonceS, nonceP, role) {
  return `${AUTH_PREFIX}|${session}|${pluginId}|${nonceS}|${nonceP}|${role}`;
}

/** 请求报文。 */
export function request(id, method, params) {
  return { t: "request", id, method, params: params ?? null };
}

/** 成功响应。 */
export function responseOk(id, result) {
  return { t: "response", id, ok: true, result: result ?? null };
}

/** 失败响应。 */
export function responseError(id, code, message) {
  return { t: "response", id, ok: false, result: null, error: { code, message } };
}

/** 单项事件。 */
export function event(topic, data) {
  return { t: "event", topic, data: data ?? null };
}

/** 心跳。 */
export function pong(at) {
  return { t: "pong", at };
}

/** 主动断开。 */
export function bye(reason) {
  return { t: "bye", reason };
}
