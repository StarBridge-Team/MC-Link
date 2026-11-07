// 示例适配器：演示 Node 轨道 SDK 的最小可用形态。
//
// 它不真的打洞——只回固定状态，用来验证"被核心拉起 → 握手 → 分发 → 响应"整条链路。
// 真实插件把 `hostStart` / `join` 换成自己的 socket / 子进程逻辑即可。
//
// 运行方式由核心负责：`<data_dir>/Runtime/bun.exe main.js --mclink-session <file>`。
// 手动调试见 `sdk/node/README.md`。
//
// 注意这里的 import 是**相对路径**（示例目录内没有 node_modules）。
// 发布成 npm 包后改成 `import { Plugin } from "@mclink/plugin-sdk"`，
// 或者用 `bun build --target=bun` 把它内联进单文件产物。

import { Plugin } from "../../src/index.js";

const plugin = await Plugin.connectFromArgs();

/** 假房间状态：示例只做状态机，不建真实隧道。 */
let phase = "waiting";
let room = "";
let port = null;

plugin.handle("adapter.init", async () => ({
  ready: true,
  // 前端据此渲染房主/访客表单；`generated` 表示由适配器生成、房主不用填
  host_fields: [
    { key: "game_port", label: "游戏端口", type: "text", required: true, placeholder: "25565" },
    { key: "room_code", label: "房间码", type: "text", generated: true },
  ],
  join_fields: [
    {
      key: "room_code",
      label: "房间码",
      type: "text",
      required: true,
      autofill_from_invite: true,
      max_length: 32,
    },
    { key: "password", label: "房间口令", type: "password", phase: "password" },
  ],
}));

plugin.handle("adapter.host.start", async (params, ctx) => {
  const requested = Number(params?.game_port ?? ctx.context.game?.port ?? 25565);
  port = Number.isFinite(requested) && requested > 0 ? requested : 25565;
  room = `ECHO-${Math.random().toString(36).slice(2, 8).toUpperCase()}`;
  phase = "host-ok";
  await ctx.log("info", `示例房间已创建: ${room}`);
  return { room, port, players: [{ name: params?.player_name ?? "host", kind: "LOCAL", machineId: "local", self: true }] };
});

plugin.handle("adapter.join", async (params, ctx) => {
  room = String(params?.room_code ?? "");
  port = 25566; // 访客侧：本机接出端口（示例固定）
  phase = "guest-ok";
  await ctx.log("info", `示例已加入: ${room}`);
  return { room, port, players: [{ name: params?.player_name ?? "guest", kind: "LOCAL", machineId: "local", self: true }] };
});

plugin.handle("adapter.host.stop", async () => {
  phase = "waiting";
  room = "";
  port = null;
  return null;
});

plugin.handle("adapter.leave", async () => {
  phase = "waiting";
  return null;
});

plugin.handle("adapter.status", async () => ({
  phase,
  port,
  room,
  players: port ? [{ name: "self", kind: "LOCAL", machineId: "local", self: true }] : [],
}));

// 插件间通信示例：别的插件可以 `core.peer.call { to: "dev.mclink.example.echo",
// method: "echo.room", params: {...} }` 来查当前房间。
plugin.onPeer("echo.room", async () => ({ room, phase, port }));

// 广播示例：收到别人的 `peer.event` 就打一条日志（不改变任何状态）。
plugin.onPeerEvent(async ({ from, topic }) => {
  await plugin.log("debug", `收到 ${from} 的 peer 事件: ${topic}`);
});

plugin.onContext((ctx) => {
  // 适配器可以在角色/房间变化时调整自己的行为；示例只记录
  console.log(`[echo-adapter] 上下文: role=${ctx.role} room=${ctx.room?.code ?? "-"}`);
});

plugin.onShutdown((reason) => {
  console.log(`[echo-adapter] 断开: ${reason}`);
});

await plugin.run();
