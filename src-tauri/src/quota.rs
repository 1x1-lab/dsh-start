//! 额度查询：按 baseURL 识别已知供应商走专用端点；未识别的 OpenAI 协议端点走通用查询。
//!
//! 端点约定参考 cc-switch（`src-tauri/src/services/balance.rs`）：
//! - 统一 GET + Bearer 鉴权 + `Accept: application/json`，15s 超时、禁用重定向；
//! - 通用查询 `{baseURL}/user/balance` 取顶层 `balance` 字段（中转站通用约定），
//!   端点 404（如 OpenAI 官方已弃用计费端点）映射为「不支持」；
//! - 金额字段兼容数字与字符串两种格式；
//! - 错误统一映射为结构化 [`QuotaError`]，消息不含 Key。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

use crate::dshconf::ProviderSummary;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// Key 长度上限（防滥用兜底）
const MAX_KEY_LEN: usize = 256;

/// 结构化错误码，前端据此映射双语提示
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaErrorCode {
    /// 输入无效（Key 含非法字符等）
    InvalidInput,
    /// 未配置凭据（apiKeyEnv 未解析到 Key）
    NoKey,
    /// 该 API 类型 / 端点不支持额度查询
    Unsupported,
    /// DSH 配置读取失败
    Config,
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
pub struct QuotaError {
    pub code: QuotaErrorCode,
    pub message: String,
}

impl QuotaError {
    pub fn new(code: QuotaErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<crate::dshconf::ConfigError> for QuotaError {
    fn from(e: crate::dshconf::ConfigError) -> Self {
        QuotaError::new(QuotaErrorCode::Config, e.message)
    }
}

/// 单个币种的余额信息；字段与 DeepSeek 官方响应保持一致
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceInfo {
    pub currency: String,
    pub total_balance: String,
    pub granted_balance: String,
    pub topped_up_balance: String,
}

/// DeepSeek 官方 `/user/balance` 响应结构（保持上游字段名）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeepSeekBalance {
    pub is_available: bool,
    pub balance_infos: Vec<BalanceInfo>,
}

/// 查询结果：按供应商类型分别承载不同的信息
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuotaResult {
    /// DeepSeek 官方：按币种逐项（金额为字符串精度，不做跨币种合计）
    DeepSeek {
        is_available: bool,
        balance_infos: Vec<BalanceInfo>,
    },
    /// OpenRouter：总额度 / 已用 / 剩余（USD）
    OpenRouter {
        total_credits: f64,
        total_usage: f64,
        remaining: f64,
    },
    /// 其他单一余额端点（StepFun / SiliconFlow / Novita / OpenAI 协议通用查询）
    Simple { balance: f64, unit: Option<String> },
}

/// 查询目标：由 baseURL 子串识别
#[derive(Debug)]
enum Target {
    DeepSeek { url: String },
    OpenRouter,
    StepFun,
    SiliconFlow { cn: bool },
    Novita,
    /// 通用查询：`{baseURL}/user/balance` → 顶层 `balance`
    Generic { url: String },
}

/// 按 baseURL 子串识别已知供应商（与线缆协议解耦）；
/// 未识别时看协议是否为 OpenAI 系，是则走通用查询，否则不支持。
fn target_for(p: &ProviderSummary) -> Result<Target, QuotaError> {
    let lower = p.base_url.as_deref().unwrap_or("").to_lowercase();
    if lower.contains("api.deepseek.com") {
        let base = p.base_url.as_deref().unwrap_or("https://api.deepseek.com");
        return Ok(Target::DeepSeek {
            url: format!("{}/user/balance", base.trim_end_matches('/')),
        });
    }
    if lower.contains("openrouter.ai") {
        return Ok(Target::OpenRouter);
    }
    if lower.contains("api.stepfun.ai") || lower.contains("api.stepfun.com") {
        return Ok(Target::StepFun);
    }
    if lower.contains("api.siliconflow.cn") {
        return Ok(Target::SiliconFlow { cn: true });
    }
    if lower.contains("api.siliconflow.com") {
        return Ok(Target::SiliconFlow { cn: false });
    }
    if lower.contains("api.novita.ai") {
        return Ok(Target::Novita);
    }

    match p.protocol.as_deref() {
        Some("openai-completions") | Some("openai-responses") => {
            let Some(base) = p.base_url.as_deref() else {
                return Err(QuotaError::new(
                    QuotaErrorCode::Unsupported,
                    "该 API 未配置 baseURL，无法查询",
                ));
            };
            Ok(Target::Generic {
                url: format!("{}/user/balance", base.trim_end_matches('/')),
            })
        }
        _ => Err(QuotaError::new(
            QuotaErrorCode::Unsupported,
            "该 API 协议暂不支持额度查询",
        )),
    }
}

/// 校验 Key：拒绝空白 / 控制字符 / 非 ASCII 等非法字符；错误消息不回显 Key。
fn validate_key(key: &str) -> Result<(), QuotaError> {
    if key.chars().any(|c| c.is_whitespace() || c.is_control() || !c.is_ascii()) {
        return Err(QuotaError::new(
            QuotaErrorCode::InvalidInput,
            "API Key 含非法字符（空白、换行、控制字符或非 ASCII）",
        ));
    }
    if key.len() > MAX_KEY_LEN {
        return Err(QuotaError::new(QuotaErrorCode::InvalidInput, "API Key 过长"));
    }
    Ok(())
}

/// HTTP 状态码 → 结构化错误
fn status_error(status: u16) -> QuotaError {
    let (code, message) = match status {
        401 | 403 => (QuotaErrorCode::AuthFailed, "鉴权失败"),
        429 => (QuotaErrorCode::RateLimited, "请求受限"),
        500..=599 => (QuotaErrorCode::Server, "服务异常"),
        _ => (QuotaErrorCode::Server, "非预期响应状态"),
    };
    QuotaError::new(code, format!("HTTP {status}: {message}"))
}

/// GET + Bearer 请求并解析 JSON；非 2xx → 状态码映射（通用查询的 404 → 不支持）。
async fn get_json(url: &str, key: &str, generic_404_unsupported: bool) -> Result<Value, QuotaError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| QuotaError::new(QuotaErrorCode::Network, format!("HTTP 客户端构建失败: {e}")))?;
    let resp = client
        .get(url)
        .bearer_auth(key)
        .header(reqwest::header::ACCEPT, "application/json")
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
            QuotaError::new(QuotaErrorCode::Network, kind)
        })?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        if generic_404_unsupported && status == 404 {
            return Err(QuotaError::new(
                QuotaErrorCode::Unsupported,
                "该端点不提供额度查询接口",
            ));
        }
        return Err(status_error(status));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|_| QuotaError::new(QuotaErrorCode::Network, "读取响应失败"))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| QuotaError::new(QuotaErrorCode::BadResponse, format!("响应解析失败: {e}")))
}

/// 金额字段：兼容数字与字符串两种格式
fn parse_f64_field(v: Option<&Value>) -> Option<f64> {
    v.and_then(|v| {
        v.as_f64()
            .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
    })
}

fn parse_deepseek(v: &Value) -> Result<QuotaResult, QuotaError> {
    let b: DeepSeekBalance = serde_json::from_value(v.clone())
        .map_err(|e| QuotaError::new(QuotaErrorCode::BadResponse, format!("响应解析失败: {e}")))?;
    Ok(QuotaResult::DeepSeek {
        is_available: b.is_available,
        balance_infos: b.balance_infos,
    })
}

fn parse_openrouter(v: &Value) -> QuotaResult {
    // data 缺失时容错回退到根对象
    let d = v.get("data").unwrap_or(v);
    let total_credits = parse_f64_field(d.get("total_credits")).unwrap_or(0.0);
    let total_usage = parse_f64_field(d.get("total_usage")).unwrap_or(0.0);
    QuotaResult::OpenRouter {
        total_credits,
        total_usage,
        remaining: total_credits - total_usage,
    }
}

fn parse_stepfun(v: &Value) -> QuotaResult {
    QuotaResult::Simple {
        balance: parse_f64_field(v.get("balance")).unwrap_or(0.0),
        unit: Some("CNY".into()),
    }
}

fn parse_siliconflow(v: &Value, cn: bool) -> Result<QuotaResult, QuotaError> {
    let d = v
        .get("data")
        .ok_or_else(|| QuotaError::new(QuotaErrorCode::BadResponse, "响应缺少 data 字段"))?;
    Ok(QuotaResult::Simple {
        balance: parse_f64_field(d.get("totalBalance")).unwrap_or(0.0),
        unit: Some(if cn { "CNY".into() } else { "USD".into() }),
    })
}

fn parse_novita(v: &Value) -> QuotaResult {
    // Novita 金额单位为 0.0001 USD，需除以 10000 转为 USD
    let raw = parse_f64_field(v.get("availableBalance")).unwrap_or(0.0);
    QuotaResult::Simple {
        balance: raw / 10000.0,
        unit: Some("USD".into()),
    }
}

fn parse_generic(v: &Value) -> Result<QuotaResult, QuotaError> {
    let balance = parse_f64_field(v.get("balance"))
        .ok_or_else(|| QuotaError::new(QuotaErrorCode::BadResponse, "响应缺少 balance 字段"))?;
    Ok(QuotaResult::Simple { balance, unit: None })
}

/// 查询某个 API 的额度。异步执行（Tauri async runtime），不阻塞 UI 线程。
pub async fn query_provider(p: &ProviderSummary, key: &str) -> Result<QuotaResult, QuotaError> {
    validate_key(key)?;
    match target_for(p)? {
        Target::DeepSeek { url } => {
            let v = get_json(&url, key, false).await?;
            parse_deepseek(&v)
        }
        Target::OpenRouter => {
            let v = get_json("https://openrouter.ai/api/v1/credits", key, false).await?;
            Ok(parse_openrouter(&v))
        }
        Target::StepFun => {
            let v = get_json("https://api.stepfun.com/v1/accounts", key, false).await?;
            Ok(parse_stepfun(&v))
        }
        Target::SiliconFlow { cn } => {
            let url = if cn {
                "https://api.siliconflow.cn/v1/user/info"
            } else {
                "https://api.siliconflow.com/v1/user/info"
            };
            let v = get_json(url, key, false).await?;
            parse_siliconflow(&v, cn)
        }
        Target::Novita => {
            let v = get_json("https://api.novita.ai/v3/user/balance", key, false).await?;
            Ok(parse_novita(&v))
        }
        Target::Generic { url } => {
            let v = get_json(&url, key, true).await?;
            parse_generic(&v)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(base_url: Option<&str>, protocol: Option<&str>) -> ProviderSummary {
        ProviderSummary {
            id: "test".into(),
            display_name: "Test".into(),
            protocol: protocol.map(Into::into),
            base_url: base_url.map(Into::into),
            api_key_env: "TEST_KEY".into(),
            key_configured: true,
            masked_key: None,
        }
    }

    #[test]
    fn target_detects_known_providers_by_base_url() {
        assert!(matches!(
            target_for(&provider(Some("https://api.deepseek.com"), None)).unwrap(),
            Target::DeepSeek { .. }
        ));
        // 显式带路径的 baseURL 也能识别
        assert!(matches!(
            target_for(&provider(Some("https://api.deepseek.com/v1"), Some("openai-completions"))).unwrap(),
            Target::DeepSeek { .. }
        ));
        assert!(matches!(
            target_for(&provider(Some("https://openrouter.ai/api/v1"), None)).unwrap(),
            Target::OpenRouter
        ));
        assert!(matches!(
            target_for(&provider(Some("https://api.stepfun.com/v1"), None)).unwrap(),
            Target::StepFun
        ));
        assert!(matches!(
            target_for(&provider(Some("https://api.stepfun.ai/v1"), None)).unwrap(),
            Target::StepFun
        ));
        assert!(matches!(
            target_for(&provider(Some("https://api.siliconflow.cn/v1"), None)).unwrap(),
            Target::SiliconFlow { cn: true }
        ));
        assert!(matches!(
            target_for(&provider(Some("https://api.siliconflow.com/v1"), None)).unwrap(),
            Target::SiliconFlow { cn: false }
        ));
        assert!(matches!(
            target_for(&provider(Some("https://api.novita.ai/v3"), None)).unwrap(),
            Target::Novita
        ));
    }

    #[test]
    fn target_generic_requires_openai_protocol() {
        let t = target_for(&provider(Some("https://relay.example.com/v1"), Some("openai-completions")))
            .unwrap();
        assert!(matches!(t, Target::Generic { .. }));
        let t = target_for(&provider(Some("https://relay.example.com/v1"), Some("openai-responses")))
            .unwrap();
        assert!(matches!(t, Target::Generic { .. }));
        // OpenAI 官方目录路由：专用计费端点已弃用 → 走通用查询，由 404 映射为不支持
        let t = target_for(&provider(Some("https://api.openai.com/v1"), Some("openai-responses")))
            .unwrap();
        assert!(matches!(t, Target::Generic { .. }));
    }

    #[test]
    fn target_unsupported_for_anthropic_and_unknown() {
        let err = target_for(&provider(Some("https://api.kimi.com/coding"), Some("anthropic-messages")))
            .unwrap_err();
        assert_eq!(err.code, QuotaErrorCode::Unsupported);
        let err = target_for(&provider(Some("https://relay.example.com"), None)).unwrap_err();
        assert_eq!(err.code, QuotaErrorCode::Unsupported);
    }

    #[test]
    fn validate_key_rejects_illegal_chars() {
        for bad in ["sk-a\nb", "sk-a b", "sk-中文"] {
            let err = validate_key(bad).unwrap_err();
            assert_eq!(err.code, QuotaErrorCode::InvalidInput, "input: {bad:?}");
        }
        assert!(validate_key("sk-0123456789abcdef").is_ok());
    }

    #[test]
    fn error_message_never_echoes_key() {
        let err = validate_key("sk-leak-me secret\n").unwrap_err();
        assert!(!err.message.contains("sk-leak-me"));
    }

    #[test]
    fn parse_deepseek_responses() {
        let body = r#"{
            "is_available": true,
            "balance_infos": [
                {"currency": "CNY", "total_balance": "110.00", "granted_balance": "10.00", "topped_up_balance": "100.00"},
                {"currency": "USD", "total_balance": "2.50", "granted_balance": "0.50", "topped_up_balance": "2.00"}
            ]
        }"#;
        let v: Value = serde_json::from_str(body).unwrap();
        let r = parse_deepseek(&v).unwrap();
        let QuotaResult::DeepSeek { is_available, balance_infos } = r else {
            panic!("wrong variant");
        };
        assert!(is_available);
        assert_eq!(balance_infos.len(), 2);
        assert_eq!(balance_infos[0].currency, "CNY");
        assert_eq!(balance_infos[0].total_balance, "110.00");

        // 空数组合法
        let v: Value = serde_json::from_str(r#"{"is_available": false, "balance_infos": []}"#).unwrap();
        let QuotaResult::DeepSeek { is_available, balance_infos } = parse_deepseek(&v).unwrap() else {
            panic!("wrong variant");
        };
        assert!(!is_available);
        assert!(balance_infos.is_empty());

        // 结构不符 → bad_response
        let v: Value = serde_json::from_str(r#"{"is_available": true}"#).unwrap();
        assert_eq!(parse_deepseek(&v).unwrap_err().code, QuotaErrorCode::BadResponse);
    }

    #[test]
    fn parse_openrouter_computes_remaining() {
        let v: Value = serde_json::from_str(
            r#"{"data": {"total_credits": 25.0, "total_usage": 17.5}}"#,
        )
        .unwrap();
        let QuotaResult::OpenRouter { total_credits, total_usage, remaining } = parse_openrouter(&v)
        else {
            panic!("wrong variant");
        };
        assert_eq!(total_credits, 25.0);
        assert_eq!(total_usage, 17.5);
        assert_eq!(remaining, 7.5);

        // data 缺失时回退根对象
        let v: Value = serde_json::from_str(r#"{"total_credits": "10", "total_usage": "4"}"#).unwrap();
        let QuotaResult::OpenRouter { remaining, .. } = parse_openrouter(&v) else {
            panic!("wrong variant");
        };
        assert_eq!(remaining, 6.0);
    }

    #[test]
    fn parse_stepfun_siliconflow_novita() {
        let v: Value = serde_json::from_str(r#"{"balance": 12.34}"#).unwrap();
        let QuotaResult::Simple { balance, unit } = parse_stepfun(&v) else {
            panic!("wrong variant");
        };
        assert_eq!(balance, 12.34);
        assert_eq!(unit.as_deref(), Some("CNY"));

        let v: Value = serde_json::from_str(r#"{"data": {"totalBalance": "88.8"}}"#).unwrap();
        let QuotaResult::Simple { balance, unit } = parse_siliconflow(&v, true).unwrap() else {
            panic!("wrong variant");
        };
        assert_eq!(balance, 88.8);
        assert_eq!(unit.as_deref(), Some("CNY"));
        let QuotaResult::Simple { unit, .. } = parse_siliconflow(&v, false).unwrap() else {
            panic!("wrong variant");
        };
        assert_eq!(unit.as_deref(), Some("USD"));
        // data 缺失 → bad_response
        let v: Value = serde_json::from_str("{}").unwrap();
        assert_eq!(
            parse_siliconflow(&v, true).unwrap_err().code,
            QuotaErrorCode::BadResponse
        );

        // Novita：0.0001 USD → USD
        let v: Value = serde_json::from_str(r#"{"availableBalance": 123456}"#).unwrap();
        let QuotaResult::Simple { balance, unit } = parse_novita(&v) else {
            panic!("wrong variant");
        };
        assert_eq!(balance, 12.3456);
        assert_eq!(unit.as_deref(), Some("USD"));
    }

    #[test]
    fn parse_generic_accepts_number_and_string() {
        let v: Value = serde_json::from_str(r#"{"balance": 42.5}"#).unwrap();
        let QuotaResult::Simple { balance, unit } = parse_generic(&v).unwrap() else {
            panic!("wrong variant");
        };
        assert_eq!(balance, 42.5);
        assert!(unit.is_none());

        let v: Value = serde_json::from_str(r#"{"balance": " 42.5 "}"#).unwrap();
        let QuotaResult::Simple { balance, .. } = parse_generic(&v).unwrap() else {
            panic!("wrong variant");
        };
        assert_eq!(balance, 42.5);

        // 缺 balance / 无法解析 → bad_response
        let v: Value = serde_json::from_str(r#"{"foo": 1}"#).unwrap();
        assert_eq!(parse_generic(&v).unwrap_err().code, QuotaErrorCode::BadResponse);
        let v: Value = serde_json::from_str(r#"{"balance": "abc"}"#).unwrap();
        assert_eq!(parse_generic(&v).unwrap_err().code, QuotaErrorCode::BadResponse);
    }

    #[test]
    fn status_error_mapping() {
        assert_eq!(status_error(401).code, QuotaErrorCode::AuthFailed);
        assert_eq!(status_error(403).code, QuotaErrorCode::AuthFailed);
        assert_eq!(status_error(429).code, QuotaErrorCode::RateLimited);
        assert_eq!(status_error(500).code, QuotaErrorCode::Server);
        assert_eq!(status_error(503).code, QuotaErrorCode::Server);
        assert_eq!(status_error(418).code, QuotaErrorCode::Server);
        for status in [401, 403, 429, 500, 418] {
            let msg = status_error(status).message;
            assert!(msg.starts_with("HTTP "), "message: {msg}");
        }
    }

    #[test]
    fn quota_result_serializes_tagged() {
        let v = serde_json::to_value(QuotaResult::Simple {
            balance: 1.0,
            unit: Some("CNY".into()),
        })
        .unwrap();
        assert_eq!(v["kind"], "simple");
        let v = serde_json::to_value(QuotaResult::DeepSeek {
            is_available: true,
            balance_infos: vec![],
        })
        .unwrap();
        assert_eq!(v["kind"], "deep_seek");
        let v = serde_json::to_value(QuotaResult::OpenRouter {
            total_credits: 1.0,
            total_usage: 0.0,
            remaining: 1.0,
        })
        .unwrap();
        assert_eq!(v["kind"], "open_router");
    }
}
