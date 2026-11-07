// 帧泵与分发：解密、派发请求/响应/事件、把异常翻译成错误码。
//
// 单独成文件的原因：它只依赖 `plugin` 的几个内部字段，却占了主类近三分之一篇幅；
// 混在 `index.js` 里会让"连接与注册"和"收包与派发"互相干扰。

import { openMessage } from "./crypto.js";
import { CoreMethod, PEER_CALL, PEER_EVENT, pong, responseError, responseOk } from "./protocol.js";

/** 连续多少帧非法后放弃（与核心 `MAX_FRAME_VIOLATIONS` 同口径）。 */
const MAX_VIOLATIONS = 8;

/** 收到一帧（握手阶段是明文 JSON，之后是加密信封）。 */
export function onText(plugin, raw) {
  if (plugin._closed) return;
  if (!plugin._ready) {
    plugin._pre.push(raw);
    return;
  }
  // 串行解密：AES-GCM 是异步的，并发解密会让消息乱序，破坏"seq 单调"这一前提
  plugin._chain = (plugin._chain ?? Promise.resolve())
    .then(() => openAndDispatch(plugin, raw))
    .catch((e) => plugin._fail(e));
}

async function openAndDispatch(plugin, raw) {
  let msg;
  try {
    const frame = JSON.parse(String(raw));
    if (frame.seq <= plugin._lastSeq) throw new Error(`检测到重放或乱序帧: ${frame.seq}`);
    plugin._lastSeq = frame.seq;
    msg = await openMessage(plugin._s2c, plugin._sid, frame);
  } catch (e) {
    plugin._violations += 1;
    if (plugin._violations >= MAX_VIOLATIONS) {
      plugin._finish(
        plugin._error("unauthorized", `连续非法帧: ${e?.message ?? e}`),
      );
    }
    return;
  }
  plugin._violations = 0;
  await dispatch(plugin, msg);
}

async function dispatch(plugin, msg) {
  switch (msg.t) {
    case "request":
      void handleRequest(plugin, msg);
      return;
    case "response": {
      const pending = plugin._pending.get(msg.id);
      if (!pending) return;
      plugin._pending.delete(msg.id);
      if (msg.ok) pending.resolve(msg.result);
      else {
        pending.reject(
          plugin._error(
            msg.error?.code ?? "internal",
            msg.error?.message ?? "核心未提供错误详情",
          ),
        );
      }
      return;
    }
    case "event":
      void handleEvent(plugin, msg);
      return;
    case "ping":
      await plugin._sendMessage(pong(msg.at));
      return;
    case "pong":
      return;
    case "bye":
      plugin._finish(null, msg.reason);
      return;
    default:
      return;
  }
}

async function handleRequest(plugin, msg) {
  const isPeer = msg.method === PEER_CALL;
  const name = isPeer ? msg.params?.method : msg.method;
  const handler = isPeer ? plugin._peerHandlers.get(name) : plugin._handlers.get(name);
  if (!handler) {
    await plugin._sendMessage(
      responseError(
        msg.id,
        "not_supported",
        isPeer ? `目标插件未实现 peer 方法: ${name}` : `插件未实现方法: ${msg.method}`,
      ),
    );
    return;
  }

  const ctx = makeContext(plugin, {
    depth: isPeer ? (msg.params?.depth ?? 0) : 0,
    from: msg.params?.from,
  });
  try {
    const result = await handler(msg.params?.params ?? null, ctx);
    await plugin._sendMessage(responseOk(msg.id, result ?? null));
  } catch (e) {
    const code = e?.code && typeof e.code === "string" ? e.code : "internal";
    await plugin._sendMessage(responseError(msg.id, code, e?.message ?? String(e)));
  }
}

async function handleEvent(plugin, msg) {
  if (msg.topic === CoreMethod.CONTEXT_UPDATE) {
    plugin._context = msg.data ?? plugin._context;
    for (const fn of plugin._contextHandlers) {
      try {
        fn(plugin._context);
      } catch (e) {
        await plugin._sendMessage({
          t: "event",
          topic: "plugin.log",
          data: { level: "error", message: `context 处理器异常: ${e?.message ?? e}` },
        });
      }
    }
    return;
  }
  if (msg.topic === PEER_EVENT) {
    for (const fn of plugin._peerEventHandlers) {
      try {
        fn(msg.data, makeContext(plugin, { depth: 0, from: msg.data?.from }));
      } catch {
        /* 订阅方自己的异常不该影响帧泵 */
      }
    }
  }
  for (const fn of plugin._eventHandlers.get(msg.topic) ?? []) {
    try {
      fn(msg.data);
    } catch {
      /* 同上 */
    }
  }
}

/**
 * 处理器拿到的上下文：绑定本次调用，避免 `depth` 串到别的调用上
 * （A→B 的第 1 层与 B 自己发起的第 0 层必须是两个值）。
 */
export function makeContext(plugin, { depth, from }) {
  return {
    pluginId: plugin.pluginId,
    from: from ?? null,
    /** 本次调用所处的 peer 嵌套深度：继续转发时 SDK 会自动带回去。 */
    depth,
    get context() {
      return plugin.context;
    },
    emit: (topic, data) => plugin.emit(topic, data),
    log: (level, message, extra) => plugin.log(level, message, extra),
    peerList: () => plugin.peerList(),
    peerCall: (to, method, params, options = {}) =>
      plugin.peerCall(to, method, params, { depth, ...options }),
    peerPublish: (topic, data, to) => plugin.peerPublish(topic, data, to),
  };
}
