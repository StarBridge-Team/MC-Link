//! # 文字聊天模块
//!
//! WebSocket 协议 + Pending Inbox 可靠投递。
//! 每条连接属于一个队伍，消息先入池再广播，接收方 ACK 后移除。

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{RwLock, mpsc};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

use crate::inbox::InboxStore;
use crate::protocol::{self, ClientMessage, PendingMessage, ServerMessage, HistoryMsg};

/// 队伍 → (客户端ID → 发送通道)
type TeamClients = Arc<RwLock<HashMap<String, HashMap<String, mpsc::UnboundedSender<Message>>>>>;

/// 处理一条已识别的 TEXT 服务 TCP 流
pub async fn handle_text_stream(
    stream: tokio::net::TcpStream,
    peer_addr: std::net::SocketAddr,
    inbox: InboxStore,
    teams: TeamClients,
) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            log::warn!("[TEXT] WebSocket 握手失败 ({}): {}", peer_addr, e);
            return;
        }
    };

    let client_id = format!("{}", peer_addr);
    log::info!("[TEXT] 新连接: {}", client_id);

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    let mut current_team: Option<String> = None;
    let mut current_sender: Option<String> = None;

    // 向客户端发送消息的任务
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    // 处理客户端消息
    while let Some(msg_result) = ws_receiver.next().await {
        match msg_result {
            Ok(msg) => {
                if msg.is_text() || msg.is_binary() {
                    let text = match msg.to_text() {
                        Ok(t) => t.to_string(),
                        Err(_) => continue,
                    };

                    let parsed: ClientMessage = match serde_json::from_str(&text) {
                        Ok(m) => m,
                        Err(_) => continue,
                    };

                    match parsed {
                        ClientMessage::Auth { team_id, sender } => {
                            // 离开旧队伍
                            if let Some(ref old_team) = current_team {
                                leave_team(&teams, old_team, &client_id).await;
                            }

                            // 加入新队伍，记录发送者昵称
                            current_team = Some(team_id.clone());
                            current_sender = Some(sender.clone());
                            join_team(&teams, &team_id, &client_id, tx.clone()).await;

                            // 拉取待收消息作为历史发送
                            let pending = inbox.drain(&team_id).await;
                            if !pending.is_empty() {
                                let history = pending.iter().map(|m| {
                                    HistoryMsg {
                                        msg_id: m.msg_id.clone(),
                                        sender: m.sender.clone(),
                                        text: m.text.clone(),
                                        time: m.time,
                                    }
                                }).collect::<Vec<_>>();

                                let hist_msg = ServerMessage::History { messages: history };
                                if let Ok(json) = serde_json::to_string(&hist_msg) {
                                    let _ = tx.send(Message::Text(json));
                                }
                            }

                            log::info!("[TEXT] {} 加入队伍 {}", sender, team_id);
                        }

                        ClientMessage::Chat { msg_id, text } => {
                            let team_id = match &current_team {
                                Some(t) => t.clone(),
                                None => continue,
                            };
                            let sender = match &current_sender {
                                Some(s) => s.clone(),
                                None => continue,
                            };
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs();

                            // 构造待收消息
                            let pending = PendingMessage {
                                msg_id: msg_id.clone(),
                                sender: sender.clone(),
                                text: text.clone(),
                                time: now,
                                created_at: std::time::Instant::now(),
                                ttl: std::time::Duration::from_secs(protocol::MESSAGE_TTL_SECS),
                            };

                            // 存入待收池（返回完整的 pending）
                            let stored = inbox.insert(&team_id, pending).await;

                            // 广播给队伍中除发送者外的所有成员
                            let broadcast = ServerMessage::Chat {
                                msg_id: stored.msg_id.clone(),
                                sender: stored.sender.clone(),
                                text: stored.text.clone(),
                                time: stored.time,
                                from_history: false,
                            };

                            if let Ok(json) = serde_json::to_string(&broadcast) {
                                broadcast_to_team(&teams, &team_id, &client_id, &json).await;
                            }
                        }

                        ClientMessage::Ack { msg_ids } => {
                            if let Some(ref team_id) = current_team {
                                let removed = inbox.acknowledge(team_id, &msg_ids).await;
                                if removed > 0 {
                                    let ack_ok = ServerMessage::AckOk {
                                        msg_ids: msg_ids.clone(),
                                    };
                                    if let Ok(json) = serde_json::to_string(&ack_ok) {
                                        let _ = tx.send(Message::Text(json));
                                    }
                                }
                            }
                        }

                        ClientMessage::Leave => {
                            if let Some(ref team_id) = current_team {
                                leave_team(&teams, team_id, &client_id).await;
                                log::info!("[TEXT] {} 离开队伍 {}", client_id, team_id);
                                current_team = None;
                            }
                        }
                    }
                } else if msg.is_close() {
                    break;
                }
            }
            Err(e) => {
                log::warn!("[TEXT] WebSocket 错误 ({}): {}", client_id, e);
                break;
            }
        }
    }

    // 断开清理
    if let Some(ref team_id) = current_team {
        leave_team(&teams, team_id, &client_id).await;
    }

    send_task.abort();
    log::info!("[TEXT] 断开: {}", client_id);
}

// ===== 队伍管理 =====

async fn join_team(
    teams: &TeamClients,
    team_id: &str,
    client_id: &str,
    tx: mpsc::UnboundedSender<Message>,
) {
    let mut map = teams.write().await;
    map.entry(team_id.to_string())
        .or_default()
        .insert(client_id.to_string(), tx);
}

async fn leave_team(teams: &TeamClients, team_id: &str, client_id: &str) {
    let mut map = teams.write().await;
    if let Some(clients) = map.get_mut(team_id) {
        clients.remove(client_id);
        if clients.is_empty() {
            map.remove(team_id);
        }
    }
}

/// 向队伍中除发送者外的所有成员广播 JSON 消息
async fn broadcast_to_team(
    teams: &TeamClients,
    team_id: &str,
    exclude: &str,
    json: &str,
) {
    let map = teams.read().await;
    if let Some(clients) = map.get(team_id) {
        for (cid, tx) in clients {
            if cid != exclude {
                let _ = tx.send(Message::Text(json.to_string()));
            }
        }
    }
}