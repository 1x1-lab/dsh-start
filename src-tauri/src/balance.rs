//! DeepSeek 账户余额查询：直接请求官方 `/user/balance` 接口，独立于 DSH 进程。
//!
//! 设计约束：
//! - 固定官方 HTTPS 地址、禁用重定向、15s 超时，异步执行不阻塞 UI；
//! - 金额保持接口返回的字符串精度，多币种全部保留（不做浮点合计）；
//! - 错误统一映射为结构化 [`BalanceError`]，消息不包含 Key 与完整请求头。

use serde::{Deserialize, Serialize};
use std::time::Duration;

const BALANCE_URL: &str = "https://api.deepseek.com/user/balance";
/// 官方余额接口请求超时
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// Key 长度上限（DeepSeek Key 为 sk- + 32 位十六进制，远短于此；仅作防滥用兜底）
const MAX_KEY_LEN: usize = 256;

/// 单个币种的余额信息；字段与官方响应保持一致
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceInfo {
    pub currency: String,
    pub total_balance: String,
    pub granted_balance: String,
    pub topped_up_balance: String,
}

/// 官方 `/user/balance` 响应结构（保持上游字段名）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeepSeekBalance {
    pub is_available: bool,
    pub balance_infos: Vec<BalanceInfo>,
}

/// 结构化错误码，前端据此映射双语提示
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BalanceErrorCode {
    /// 输入无效（空、含空白 / 控制字符 / 非 ASCII 等）
    InvalidInput,
    /// 鉴权失败（HTTP 401 / 403）
    AuthFailed,
    /// 请求受限（HTTP 429）
    RateLimited,
    /// 网络错误或超时
    Network,
    /// 服务异常（HTTP 5xx 及其他非预期状态）
    Server,
    /// 响应格式异常（状态 2xx 但 JSON 不符合结构）
    BadResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct BalanceError {
    pub code: BalanceErrorCode,
    pub message: String,
}

impl BalanceError {
    fn new(code: BalanceErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// 校验并规范化 Key：去除首尾空白；拒绝空值与空白 / 控制字符 / 非 ASCII 等非法字符。
/// 错误消息只描述原因，不回显 Key 内容。
pub fn validate_key(raw: &str) -> Result<String, BalanceError> {
    let key = raw.trim();
    if key.is_empty() {
        return Err(BalanceError::new(
            BalanceErrorCode::InvalidInput,
            "API Key 为空",
        ));
    }
    if key
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || !c.is_ascii())
    {
        return Err(BalanceError::new(
            BalanceErrorCode::InvalidInput,
            "API Key 含非法字符（空白、换行、控制字符或非 ASCII）",
        ));
    }
    if key.len() > MAX_KEY_LEN {
        return Err(BalanceError::new(
            BalanceErrorCode::InvalidInput,
            "API Key 过长",
        ));
    }
    Ok(key.to_string())
}

/// HTTP 状态码 → 结构化错误
fn status_error(status: u16) -> BalanceError {
    let (code, message) = match status {
        401 | 403 => (BalanceErrorCode::AuthFailed, "鉴权失败"),
        429 => (BalanceErrorCode::RateLimited, "请求受限"),
        500..=599 => (BalanceErrorCode::Server, "服务异常"),
        _ => (BalanceErrorCode::Server, "非预期响应状态"),
    };
    BalanceError::new(code, format!("HTTP {status}: {message}"))
}

/// 解析响应：非 2xx → 状态码映射；2xx 但 JSON 不符合结构 → 格式异常。
/// `balance_infos` 为空数组视为合法（由前端展示空结果状态）。
pub fn parse_response(status: u16, body: &str) -> Result<DeepSeekBalance, BalanceError> {
    if !(200..300).contains(&status) {
        return Err(status_error(status));
    }
    serde_json::from_str(body)
        .map_err(|e| BalanceError::new(BalanceErrorCode::BadResponse, format!("响应解析失败: {e}")))
}

/// 请求官方余额接口。异步执行（Tauri async runtime），不阻塞 UI 线程。
pub async fn fetch_balance(api_key: &str) -> Result<DeepSeekBalance, BalanceError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| BalanceError::new(BalanceErrorCode::Network, format!("HTTP 客户端构建失败: {e}")))?;
    let resp = client
        .get(BALANCE_URL)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| {
            // 只取错误类别，不透传原始错误（避免把请求上下文带进消息）
            let kind = if e.is_timeout() {
                "请求超时"
            } else if e.is_connect() {
                "连接失败"
            } else {
                "网络错误"
            };
            BalanceError::new(BalanceErrorCode::Network, kind)
        })?;
    let status = resp.status().as_u16();
    let body = resp
        .text()
        .await
        .map_err(|_| BalanceError::new(BalanceErrorCode::Network, "读取响应失败"))?;
    parse_response(status, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_key_trims_and_accepts() {
        assert_eq!(validate_key("  sk-abc123 \t\n").unwrap(), "sk-abc123");
        assert_eq!(validate_key("sk-0123456789abcdef").unwrap(), "sk-0123456789abcdef");
    }

    #[test]
    fn validate_key_rejects_empty_and_illegal_chars() {
        for bad in ["", "   ", "\n\t ", "sk-a\nb", "sk-a b", "sk-a\tb", "sk-中文"] {
            let err = validate_key(bad).unwrap_err();
            assert_eq!(err.code, BalanceErrorCode::InvalidInput, "input: {bad:?}");
        }
    }

    #[test]
    fn error_message_never_echoes_key() {
        let err = validate_key("sk-leak-me secret\n").unwrap_err();
        assert!(!err.message.contains("sk-leak-me"));
    }

    #[test]
    fn parse_normal_balance() {
        let body = r#"{
            "is_available": true,
            "balance_infos": [
                {"currency": "CNY", "total_balance": "110.00", "granted_balance": "10.00", "topped_up_balance": "100.00"}
            ]
        }"#;
        let b = parse_response(200, body).unwrap();
        assert!(b.is_available);
        assert_eq!(b.balance_infos.len(), 1);
        assert_eq!(b.balance_infos[0].currency, "CNY");
        assert_eq!(b.balance_infos[0].total_balance, "110.00");
        assert_eq!(b.balance_infos[0].granted_balance, "10.00");
        assert_eq!(b.balance_infos[0].topped_up_balance, "100.00");
    }

    #[test]
    fn parse_zero_balance() {
        let body = r#"{"is_available": false, "balance_infos": [
            {"currency": "CNY", "total_balance": "0.00", "granted_balance": "0.00", "topped_up_balance": "0.00"}
        ]}"#;
        let b = parse_response(200, body).unwrap();
        assert!(!b.is_available);
        assert_eq!(b.balance_infos[0].total_balance, "0.00");
    }

    #[test]
    fn parse_multi_currency_keeps_all_items() {
        let body = r#"{"is_available": true, "balance_infos": [
            {"currency": "CNY", "total_balance": "1.00", "granted_balance": "0.00", "topped_up_balance": "1.00"},
            {"currency": "USD", "total_balance": "2.50", "granted_balance": "0.50", "topped_up_balance": "2.00"}
        ]}"#;
        let b = parse_response(200, body).unwrap();
        assert_eq!(b.balance_infos.len(), 2);
        assert_eq!(b.balance_infos[0].currency, "CNY");
        assert_eq!(b.balance_infos[1].currency, "USD");
        // 字符串精度原样保留
        assert_eq!(b.balance_infos[1].granted_balance, "0.50");
    }

    #[test]
    fn parse_empty_balance_array_is_valid() {
        let b = parse_response(200, r#"{"is_available": false, "balance_infos": []}"#).unwrap();
        assert!(!b.is_available);
        assert!(b.balance_infos.is_empty());
    }

    #[test]
    fn parse_maps_auth_failure_and_rate_limit() {
        assert_eq!(parse_response(401, "{}").unwrap_err().code, BalanceErrorCode::AuthFailed);
        assert_eq!(parse_response(403, "").unwrap_err().code, BalanceErrorCode::AuthFailed);
        assert_eq!(parse_response(429, "").unwrap_err().code, BalanceErrorCode::RateLimited);
    }

    #[test]
    fn parse_maps_server_errors() {
        assert_eq!(parse_response(500, "").unwrap_err().code, BalanceErrorCode::Server);
        assert_eq!(parse_response(503, "").unwrap_err().code, BalanceErrorCode::Server);
        assert_eq!(parse_response(404, "").unwrap_err().code, BalanceErrorCode::Server);
    }

    #[test]
    fn parse_maps_malformed_json_to_bad_response() {
        // 非 JSON 文本
        assert_eq!(parse_response(200, "not json").unwrap_err().code, BalanceErrorCode::BadResponse);
        // 字段缺失
        assert_eq!(
            parse_response(200, r#"{"is_available": true}"#).unwrap_err().code,
            BalanceErrorCode::BadResponse
        );
        // 字段类型错误（金额应为字符串）
        assert_eq!(
            parse_response(200, r#"{"is_available": "yes", "balance_infos": []}"#)
                .unwrap_err()
                .code,
            BalanceErrorCode::BadResponse
        );
        assert_eq!(
            parse_response(
                200,
                r#"{"is_available": true, "balance_infos": [{"currency": "CNY", "total_balance": 1.0, "granted_balance": "0", "topped_up_balance": "0"}]}"#
            )
            .unwrap_err()
            .code,
            BalanceErrorCode::BadResponse
        );
    }

    #[test]
    fn status_error_messages_do_not_leak_secrets() {
        for status in [401, 403, 429, 500, 418] {
            let err = status_error(status);
            assert!(err.message.starts_with("HTTP "));
        }
    }
}
