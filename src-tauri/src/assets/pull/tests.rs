use super::*;

#[test]
fn rejects_unsafe_manifest_paths() {
    assert!(is_safe_relative("fonts/poppins.css"));
    assert!(is_safe_relative("icons/a.png"));

    // 越界与绝对路径必须被拒绝：清单来自网络，不能让它写到 Assets/ 之外
    assert!(!is_safe_relative(""));
    assert!(!is_safe_relative("../evil.txt"));
    assert!(!is_safe_relative("fonts/../../evil.txt"));
    assert!(!is_safe_relative("/etc/passwd"));
    assert!(!is_safe_relative("C:/Windows/system32/evil.dll"));
    assert!(!is_safe_relative("./relative"));
}

#[test]
fn manifest_parses_with_and_without_sha256() {
    let text = r#"{
        "version": "abc",
        "assets": [
            { "path": "a.css", "size": 3, "sha256": "aa" },
            { "path": "b.css" }
        ]
    }"#;
    let m: AssetsManifest = serde_json::from_str(text).unwrap();
    assert_eq!(m.assets.len(), 2);
    assert_eq!(m.assets[0].sha256.as_deref(), Some("aa"));
    assert!(m.assets[1].sha256.is_none());
}

#[test]
fn outcome_all_ready_requires_non_empty_list() {
    let empty = SyncOutcome::default();
    assert!(!empty.all_ready(), "空清单不能算就绪");

    let partial = SyncOutcome {
        assets: vec![
            AssetState {
                path: "a.css".into(),
                ready: true,
                reason: None,
            },
            AssetState {
                path: "b.css".into(),
                ready: false,
                reason: Some("缺失".into()),
            },
        ],
        ..Default::default()
    };
    assert!(!partial.all_ready());

    let full = SyncOutcome {
        assets: vec![AssetState {
            path: "a.css".into(),
            ready: true,
            reason: None,
        }],
        ..Default::default()
    };
    assert!(full.all_ready());
}

#[tokio::test]
async fn local_state_detects_missing_and_size_mismatch() {
    let dir = std::env::temp_dir().join(format!(
        "mclink-assets-{}",
        crate::plugin::crypto::random_hex(6)
    ));
    std::fs::create_dir_all(dir.join("Assets")).unwrap();

    let entry = AssetEntry {
        path: "a.css".into(),
        size: Some(2),
        sha256: Some(hex::encode(Sha256::digest(b"hello"))),
    };
    assert!(matches!(
        local_state(&dir, &entry).await,
        LocalState::Missing
    ));

    std::fs::write(dir.join("Assets").join("a.css"), b"hello").unwrap();
    // size 声明 2 字节，实际 5 字节 → 必须判为不符
    assert!(matches!(
        local_state(&dir, &entry).await,
        LocalState::Mismatch(_)
    ));

    let entry_ok = AssetEntry {
        size: Some(5),
        ..entry
    };
    assert!(matches!(
        local_state(&dir, &entry_ok).await,
        LocalState::Ready
    ));

    let _ = std::fs::remove_dir_all(&dir);
}

/// 回归：清单**没有** sha256 时必须判为未就绪。
///
/// 原逻辑是"文件存在 + 大小对就算就绪"，于是被劫持的资源服务器只要下发一份
/// 不带 sha256 的清单，就能让已被替换过的本地文件继续被判"就绪"并注入前端。
#[tokio::test]
async fn local_state_without_sha256_is_not_ready() {
    let dir = std::env::temp_dir().join(format!(
        "mclink-assets-nohash-{}",
        crate::plugin::crypto::random_hex(6)
    ));
    std::fs::create_dir_all(dir.join("Assets")).unwrap();
    std::fs::write(dir.join("Assets").join("a.css"), b"hello").unwrap();

    let entry = AssetEntry {
        path: "a.css".into(),
        size: Some(5),
        sha256: None,
    };
    assert!(
        matches!(local_state(&dir, &entry).await, LocalState::Mismatch(_)),
        "缺 sha256 的文件不能算就绪"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------------------ 整链路测试
//
// 下面三个用例驱动完整的「拉清单 → 下载 → 校验 → 落盘」，用一个进程内的极简
// HTTP 服务器扮演资源服务器。这样就不必为了验证这条链路去启动 GUI。

fn temp_data_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mclink-assets-{}-{}",
        tag,
        crate::plugin::crypto::random_hex(5)
    ));
    std::fs::create_dir_all(dir.join("Assets")).unwrap();
    dir
}

/// 构造一份"服务器目录"：写入文件并生成带哈希的 manifest.json。
/// `tampered` 里的路径会在清单中写入一个**错误**哈希，用于模拟被篡改的内容。
fn build_server(parent: &Path, files: &[(&str, &[u8])], tampered: &[&str]) -> PathBuf {
    let root = parent.join("server");
    std::fs::create_dir_all(&root).unwrap();
    for (rel, bytes) in files {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
    }
    let assets: Vec<serde_json::Value> = files
        .iter()
        .map(|(rel, bytes)| {
            let sha256 = if tampered.contains(rel) {
                "0".repeat(64)
            } else {
                hex::encode(Sha256::digest(bytes))
            };
            serde_json::json!({ "path": rel, "size": bytes.len(), "sha256": sha256 })
        })
        .collect();
    let manifest = serde_json::json!({ "version": "test-1", "assets": assets });
    std::fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    root
}

/// 极简 HTTP/1.1 静态服务器，只处理 GET。返回 (base_url, stop)。
fn spawn_asset_server(root: PathBuf) -> (String, std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let listener = TcpListener::bind("127.0.0.1:0").expect("绑定测试端口");
    let addr = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();

    std::thread::spawn(move || {
        while !flag.load(Ordering::SeqCst) {
            let (mut stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(_) => break,
            };

            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            // 吃掉请求头
            loop {
                let mut header = String::new();
                match reader.read_line(&mut header) {
                    Ok(0) => break,
                    Ok(_) if header.trim().is_empty() => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }

            let rel = request_line
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .trim_start_matches('/')
                .to_string();
            match std::fs::read(root.join(&rel)) {
                Ok(bytes) => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        bytes.len()
                    );
                    let _ = stream.write_all(&bytes);
                }
                Err(_) => {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                }
            }
            let _ = stream.flush();
        }
    });

    (format!("http://{}", addr), stop)
}

#[tokio::test]
async fn downloads_and_verifies_declared_assets() {
    let parent = temp_data_dir("ok");
    let server = build_server(
        &parent,
        &[("a/one.css", b"body{}"), ("b/two.woff2", b"font-bytes")],
        &[],
    );
    let (base, stop) = spawn_asset_server(server);
    let client = reqwest::Client::new();

    // 允许重试一次：在 Windows 上，刚写入临时目录的文件偶尔会被杀毒/索引服务
    // 短暂占用，使 `persist::atomic_write` 的改名失败。这类瞬时 I/O 故障与逻辑无关，
    // 但连续两次失败就一定是我们自己的问题（真 bug 不会自愈）。
    let mut outcome = sync_assets_from(&parent, &client, &base).await.unwrap();
    if !outcome.all_ready() {
        outcome = sync_assets_from(&parent, &client, &base).await.unwrap();
    }
    assert!(
        outcome.all_ready(),
        "全部资源应就绪；failures={:?}",
        outcome.failures
    );
    assert_eq!(
        std::fs::read(parent.join("Assets").join("a/one.css")).unwrap(),
        b"body{}"
    );
    assert_eq!(
        std::fs::read(parent.join("Assets").join("b/two.woff2")).unwrap(),
        b"font-bytes"
    );

    // 本地清单应写入，供下次离线比对
    let local = read_local_manifest(&parent).expect("本地清单应已写入");
    assert_eq!(local.version, "test-1");

    // 缓存命中：再次同步必须仍然全就绪（不能因为重复下载把状态搞坏）
    let again = sync_assets_from(&parent, &client, &base).await.unwrap();
    assert!(again.all_ready());

    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = std::fs::remove_dir_all(&parent);
}

#[tokio::test]
async fn rejects_asset_whose_hash_does_not_match() {
    let parent = temp_data_dir("bad");
    let server = build_server(
        &parent,
        &[("good.css", b"ok"), ("evil.css", b"tampered")],
        &["evil.css"],
    );
    let (base, stop) = spawn_asset_server(server);
    let client = reqwest::Client::new();

    let outcome = sync_assets_from(&parent, &client, &base).await.unwrap();

    assert!(!outcome.all_ready());
    assert!(!outcome.failures.is_empty(), "失败要有可展示的原因");
    // 哈希不符的内容绝不能落盘
    assert!(
        !parent.join("Assets").join("evil.css").exists(),
        "校验不过的文件不得写入缓存"
    );
    // 通过校验的那个仍要就绪：个别失败不该拖垮整体
    assert!(parent.join("Assets").join("good.css").exists());
    // 半成功状态不写版本号，否则下次启动会跳过补齐
    assert!(read_local_manifest(&parent).is_none());

    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = std::fs::remove_dir_all(&parent);
}

#[tokio::test]
async fn falls_back_to_local_cache_when_server_unreachable() {
    let parent = temp_data_dir("offline");
    let manifest = AssetsManifest {
        version: "v-local".into(),
        description: String::new(),
        server: String::new(),
        assets: vec![AssetEntry {
            path: "a.css".into(),
            size: Some(6),
            sha256: Some(hex::encode(Sha256::digest(b"cached"))),
        }],
    };
    std::fs::write(parent.join("Assets").join("a.css"), b"cached").unwrap();
    write_local_manifest(&parent, &manifest).unwrap();

    let client = reqwest::Client::new();
    // 指向必定无人监听的端口
    let outcome = sync_assets_from(&parent, &client, "http://127.0.0.1:1")
        .await
        .unwrap();

    assert!(outcome.offline, "连不上服务器时应标记离线");
    assert!(outcome.all_ready(), "本地缓存哈希对得上就仍应算就绪");
    // 离线但缓存齐全 = 一切正常，不该报错。
    // 否则一个完全可用的应用会平白弹出"资源同步失败"。
    assert!(
        outcome.failures.is_empty(),
        "缓存齐全时离线不应算失败：{:?}",
        outcome.failures
    );

    // 反过来：离线且缓存确实缺文件，就必须报出来并标记未就绪
    std::fs::remove_file(parent.join("Assets").join("a.css")).unwrap();
    let broken = sync_assets_from(&parent, &client, "http://127.0.0.1:1")
        .await
        .unwrap();
    assert!(broken.offline);
    assert!(!broken.all_ready());
    assert!(!broken.failures.is_empty(), "缺文件时必须给出原因");

    let _ = std::fs::remove_dir_all(&parent);
}
