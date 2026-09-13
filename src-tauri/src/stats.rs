//! 读取 DSH 会话数据，聚合 token 用量统计：
//! - 会话投影缓存（`storages/session_projcache`，明文 JSON）→ 每会话汇总；
//! - 会话日志（`sessions/*/*/session.jsonl.zstd` 或 `session.v3.jsonl.zstd`，多帧 zstd 拼接）→ 按时间的用量序列。

use serde::Serialize;
use serde_json::Value;
use std::path::Path;

/// 单个会话的 token 用量
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenSessionUsage {
    /// 会话标题（DSH 自动生成，可能为空）
    pub title: Option<String>,
    /// 会话所属项目目录
    pub cwd: String,
    /// 会话创建时间（毫秒时间戳）
    pub created_at: Option<u64>,
    pub turns: u64,
    /// 未命中缓存的输入 token
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

fn u64_field(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// 一个用量结算点（assistant usage 事件,带毫秒时间戳）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsagePoint {
    /// 事件时间（毫秒时间戳）
    pub ts: i64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

/// 解压单个会话日志（多帧 zstd 拼接,decode_all 会连续跨越帧边界）
fn decompress_session_log(path: &Path) -> Result<String, String> {
    let raw = std::fs::read(path).map_err(|e| format!("{} 读取失败: {e}", path.display()))?;
    let text = zstd::stream::decode_all(raw.as_slice())
        .map_err(|e| format!("{} 解压失败: {e}", path.display()))?;
    Ok(String::from_utf8_lossy(&text).into_owned())
}

#[derive(Clone, Copy)]
enum SessionLogFormat {
    Legacy,
    V3,
}

/// 按文件版本提取 usage：旧文件只读 chunk，新版只读 assistant/message，避免重复计数。
fn extract_usage_points(text: &str, format: SessionLogFormat) -> Vec<UsagePoint> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let data = v.get("data");
        let usage = match (format, v.get("type").and_then(Value::as_str)) {
            (SessionLogFormat::Legacy, Some("assistant/chunk")) => {
                let Some(chunk) = data.and_then(|d| d.get("chunk")) else {
                    continue;
                };
                if chunk.get("type").and_then(Value::as_str) != Some("usage") {
                    continue;
                }
                chunk.get("usage")
            }
            (SessionLogFormat::V3, Some("assistant/message")) => data.and_then(|d| d.get("usage")),
            _ => None,
        };
        let Some(usage) = usage.filter(|usage| usage.is_object()) else {
            continue;
        };
        let Some(ts) = v.get("time").and_then(Value::as_i64) else {
            continue;
        };
        out.push(UsagePoint {
            ts,
            input_tokens: u64_field(usage, "inputTokens"),
            output_tokens: u64_field(usage, "outputTokens"),
            cache_read_tokens: u64_field(usage, "cacheReadTokens"),
            cache_write_tokens: u64_field(usage, "cacheWriteTokens"),
        });
    }
    out
}

/// New DSH versions write `session.v3.jsonl.zstd`; older sessions still use
/// `session.jsonl.zstd`. Read only one file per session, preferring v3 if both exist.
fn session_log_path(session: &Path) -> Option<(std::path::PathBuf, SessionLogFormat)> {
    let v3 = session.join("session.v3.jsonl.zstd");
    if v3.is_file() {
        return Some((v3, SessionLogFormat::V3));
    }
    let legacy = session.join("session.jsonl.zstd");
    legacy.is_file().then_some((legacy, SessionLogFormat::Legacy))
}

/// 汇总 [start_ms, end_ms] 范围内全部会话的用量结算点，按时间升序。
/// 日志 mtime 早于 start 的文件直接跳过（不可能包含范围内的数据）。
pub fn token_usage_series(dsh_home: &Path, start_ms: i64, end_ms: i64) -> Vec<UsagePoint> {
    let root = dsh_home.join("sessions");
    let Ok(project_dirs) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for proj in project_dirs.flatten() {
        let Ok(session_dirs) = std::fs::read_dir(proj.path()) else {
            continue;
        };
        for session in session_dirs.flatten() {
            let Some((log, format)) = session_log_path(&session.path()) else {
                continue;
            };
            let mtime = std::fs::metadata(&log)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64);
            if let Some(mt) = mtime {
                if mt < start_ms {
                    continue;
                }
            }
            let Ok(text) = decompress_session_log(&log) else {
                continue;
            };
            out.extend(
                extract_usage_points(&text, format)
                    .into_iter()
                    .filter(|p| p.ts >= start_ms && p.ts <= end_ms),
            );
        }
    }
    out.sort_by_key(|p| p.ts);
    out
}

/// 从一个投影缓存 JSON 提取会话用量；缺行/缺字段按 0 处理，结构完全不符时跳过
fn session_usage_from_json(v: &Value) -> Option<TokenSessionUsage> {
    let record = v.get("record")?;
    let identity = record.get("identity")?;
    let cwd = identity
        .get("cwd")
        .and_then(Value::as_str)?
        .trim()
        .to_string();
    if cwd.is_empty() {
        return None;
    }
    let rows = record.get("rows")?;
    let totals = rows
        .get("tokenUsage")
        .and_then(|r| r.get("val"))
        .and_then(|u| u.get("totals"));
    let turns = rows
        .get("sessionStats")
        .and_then(|r| r.get("val"))
        .map(|s| u64_field(s, "turns"))
        .unwrap_or(0);
    let title = rows
        .get("title")
        .and_then(|r| r.get("val"))
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some(TokenSessionUsage {
        title,
        cwd,
        created_at: identity.get("createdAt").and_then(Value::as_u64),
        turns,
        input_tokens: totals.map(|t| u64_field(t, "uncachedInputTokens")).unwrap_or(0),
        output_tokens: totals.map(|t| u64_field(t, "outputTokens")).unwrap_or(0),
        cache_read_tokens: totals.map(|t| u64_field(t, "cacheReadTokens")).unwrap_or(0),
        cache_write_tokens: totals.map(|t| u64_field(t, "cacheWriteTokens")).unwrap_or(0),
    })
}

/// 全部会话的 token 用量，按创建时间升序（时间轴）。
/// 目录不存在（未安装 DSH）返回空列表而非错误。
pub fn token_usage_sessions(dsh_home: &Path) -> Vec<TokenSessionUsage> {
    let dir = dsh_home
        .join("storages")
        .join("session_projcache")
        .join("sessions");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        // DSH 侧为原子写，但仍容错：单个文件读/解析失败只跳过该会话
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if let Some(s) = session_usage_from_json(&v) {
            sessions.push(s);
        }
    }
    sessions.sort_by_key(|s| s.created_at.unwrap_or(0));
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机诊断：汇总当天真实会话日志，帮助人工核对序列提取。
    #[test]
    #[ignore]
    fn inspect_real_series() {
        let home = crate::dshconf::dsh_home();
        let today = chrono::Local::now().date_naive();
        let start = today
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Local)
            .earliest()
            .unwrap()
            .timestamp_millis();
        let end = today
            .succ_opt()
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Local)
            .earliest()
            .unwrap()
            .timestamp_millis()
            - 1;
        let pts = token_usage_series(&home, start, end);
        let input: u64 = pts.iter().map(|p| p.input_tokens).sum();
        let output: u64 = pts.iter().map(|p| p.output_tokens).sum();
        let cache_read: u64 = pts.iter().map(|p| p.cache_read_tokens).sum();
        let cache_write: u64 = pts.iter().map(|p| p.cache_write_tokens).sum();
        println!(
            "{}: {} points, input={input}, output={output}, cacheRead={cache_read}, cacheWrite={cache_write}",
            today,
            pts.len()
        );
    }

    #[test]
    fn extract_usage_points_uses_only_the_selected_log_schema() {
        let legacy = concat!(
            r#"{"seq":21,"time":1786775192458,"type":"request/header"}"#, "\n",
            r#"{"data":{"chunk":{"type":"text","text":"hi"}},"seq":199,"time":1786775194600,"type":"assistant/chunk"}"#, "\n",
            r#"{"data":{"chunk":{"type":"usage","usage":{"cacheReadTokens":384,"inputTokens":8138,"outputTokens":230,"reasoningTokens":119}},"step":1,"turn":1},"seq":200,"time":1786775194635,"type":"assistant/chunk"}"#, "\n",
            "not json", "\n",
            r#"{"data":{"chunk":{"type":"usage","usage":{"inputTokens":10,"outputTokens":5}},"step":2,"turn":2},"seq":300,"time":1786775294635,"type":"assistant/chunk"}"#, "\n",
            // Some legacy logs also contain a message event for the same usage.
            r#"{"data":{"turn":1,"step":2,"usage":{"inputTokens":42,"outputTokens":13,"cacheReadTokens":7,"cacheWriteTokens":3}},"seq":301,"time":1786775394635,"type":"assistant/message"}"#
        );
        let pts = extract_usage_points(legacy, SessionLogFormat::Legacy);
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0].ts, 1786775194635);
        assert_eq!(pts[0].input_tokens, 8138);
        assert_eq!(pts[0].output_tokens, 230);
        assert_eq!(pts[0].cache_read_tokens, 384);
        assert_eq!(pts[1].ts, 1786775294635);
        assert_eq!(pts[1].input_tokens, 10);
        assert_eq!(pts[1].cache_read_tokens, 0);

        let v3 = concat!(
            r#"{"data":{"turn":1,"step":2,"usage":{"inputTokens":42,"outputTokens":13,"totalTokens":55,"cacheReadTokens":7,"cacheWriteTokens":3}},"seq":301,"time":1786775394635,"type":"assistant/message"}"#, "\n",
            r#"{"data":{"usage":null},"seq":302,"time":1786775494635,"type":"assistant/message"}"#
        );
        let pts = extract_usage_points(v3, SessionLogFormat::V3);
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].ts, 1786775394635);
        assert_eq!(pts[0].input_tokens, 42);
        assert_eq!(pts[0].output_tokens, 13);
        assert_eq!(pts[0].cache_read_tokens, 7);
        assert_eq!(pts[0].cache_write_tokens, 3);
    }

    #[test]
    fn token_usage_series_filters_by_range_and_sorts() {
        let dir = std::env::temp_dir().join(format!("dsh-start-series-{}", std::process::id()));
        let session = dir.join("sessions").join("proj").join("session-x");
        std::fs::create_dir_all(&session).unwrap();
        let text = concat!(
            r#"{"data":{"chunk":{"type":"usage","usage":{"inputTokens":100,"outputTokens":10}}},"time":2000,"type":"assistant/chunk"}"#, "\n",
            r#"{"data":{"chunk":{"type":"usage","usage":{"inputTokens":50,"outputTokens":5}}},"time":1000,"type":"assistant/chunk"}"#, "\n",
            r#"{"data":{"chunk":{"type":"usage","usage":{"inputTokens":999,"outputTokens":1}}},"time":9000,"type":"assistant/chunk"}"#
        );
        let zstd_bytes = zstd::stream::encode_all(text.as_bytes(), 3).unwrap();
        std::fs::write(session.join("session.jsonl.zstd"), zstd_bytes).unwrap();

        let mut out = token_usage_series(&dir, 500, 3000);
        assert_eq!(out.len(), 2); // 9000 处的点被范围过滤
        assert_eq!(out[0].ts, 1000); // 升序
        assert_eq!(out[1].ts, 2000);
        assert_eq!(out[0].input_tokens, 50);

        // 范围外无数据
        out = token_usage_series(&dir, 10_000, 20_000);
        assert!(out.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn token_usage_series_supports_v3_and_legacy_files_without_duplicates() {
        let dir = std::env::temp_dir().join(format!("dsh-start-v3-series-{}", std::process::id()));
        let v3_session = dir.join("sessions").join("proj").join("session-v3");
        let legacy_session = dir.join("sessions").join("proj").join("session-old");
        std::fs::create_dir_all(&v3_session).unwrap();
        std::fs::create_dir_all(&legacy_session).unwrap();

        let v3_text = r#"{"data":{"usage":{"inputTokens":42,"outputTokens":13,"cacheReadTokens":7,"cacheWriteTokens":3}},"time":1500,"type":"assistant/message"}"#;
        let duplicate_legacy_text = r#"{"data":{"chunk":{"type":"usage","usage":{"inputTokens":999}}},"time":1600,"type":"assistant/chunk"}"#;
        let old_text = concat!(
            r#"{"data":{"chunk":{"type":"usage","usage":{"inputTokens":8,"outputTokens":4}}},"time":2500,"type":"assistant/chunk"}"#, "\n",
            r#"{"data":{"usage":{"inputTokens":8,"outputTokens":4}},"time":2500,"type":"assistant/message"}"#
        );
        std::fs::write(
            v3_session.join("session.v3.jsonl.zstd"),
            zstd::stream::encode_all(v3_text.as_bytes(), 3).unwrap(),
        )
        .unwrap();
        // If both names exist, v3 is authoritative for that session.
        std::fs::write(
            v3_session.join("session.jsonl.zstd"),
            zstd::stream::encode_all(duplicate_legacy_text.as_bytes(), 3).unwrap(),
        )
        .unwrap();
        // Old sessions that only have the original filename remain supported.
        std::fs::write(
            legacy_session.join("session.jsonl.zstd"),
            zstd::stream::encode_all(old_text.as_bytes(), 3).unwrap(),
        )
        .unwrap();

        let points = token_usage_series(&dir, 0, 3000);
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].ts, 1500);
        assert_eq!(points[0].input_tokens, 42);
        assert_eq!(points[0].cache_read_tokens, 7);
        assert_eq!(points[1].ts, 2500);
        assert_eq!(points[1].input_tokens, 8);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn session_usage_parses_projection_shape() {
        let v: Value = serde_json::from_str(
            r#"{
              "version": 5,
              "record": {
                "identity": { "createdAt": 1786774930042, "cwd": "D:\\MyProjects\\dsh-start" },
                "rows": {
                  "sessionStats": { "ver": 1, "seq": 1, "val": { "turns": 54, "steps": 602 } },
                  "title": { "ver": 1, "seq": 1, "val": "跨端自启重启的dsh安装项目" },
                  "goal": { "ver": 4, "seq": 1, "val": null },
                  "tokenUsage": { "ver": 1, "seq": 1, "val": {
                    "totals": { "uncachedInputTokens": 746892, "outputTokens": 419059,
                                "cacheReadTokens": 224000768, "cacheWriteTokens": 0 },
                    "last": null
                  } }
                }
              }
            }"#,
        )
        .unwrap();
        let s = session_usage_from_json(&v).unwrap();
        assert_eq!(s.cwd, "D:\\MyProjects\\dsh-start");
        assert_eq!(s.title.as_deref(), Some("跨端自启重启的dsh安装项目"));
        assert_eq!(s.turns, 54);
        assert_eq!(s.input_tokens, 746892);
        assert_eq!(s.output_tokens, 419059);
        assert_eq!(s.cache_read_tokens, 224000768);
        assert_eq!(s.created_at, Some(1786774930042));
    }

    #[test]
    fn session_usage_missing_rows_defaults_to_zero() {
        let v: Value =
            serde_json::from_str(r#"{ "record": { "identity": { "cwd": "D:/x" }, "rows": {} } }"#)
                .unwrap();
        let s = session_usage_from_json(&v).unwrap();
        assert_eq!(s.turns, 0);
        assert_eq!(s.input_tokens, 0);
        assert!(s.title.is_none());
    }

    #[test]
    fn session_usage_skips_structures_without_identity() {
        assert!(session_usage_from_json(&serde_json::json!({ "foo": 1 })).is_none());
        assert!(session_usage_from_json(
            &serde_json::json!({ "record": { "identity": { "cwd": "  " } } })
        )
        .is_none());
    }
}
