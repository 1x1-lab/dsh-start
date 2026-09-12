//! 读取 DSH 用户配置（`~/.dsh`，可被 `DSH_HOME` 覆盖），列出其中全部 API（供应商路由）。
//!
//! - `settings.yaml`：固定路由 `deepseek-official` 置首，其余取 `llm-pi-ai.providers`；
//! - `.credentials.yaml`：`refs`（环境变量名 → Key 明文），解析优先级与 DSH 一致（环境变量优先）；
//! - Key 全程留在 Rust 侧，只以脱敏形式（前6…后4）返回前端；
//! - 目录路由未显式配置的 `api` / `baseURL` / `apiKeyEnv` 从 pi-ai 内置目录摘录补全。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// DSH 内置固定路由（dsh-llm-deepseek 适配器注册，`agent-default-model` 的默认指向）
pub const DEEPSEEK_OFFICIAL_ID: &str = "deepseek-official";

/// 单个 API（供应商路由）的概要；Key 只含脱敏形式。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    /// 路由键，如 `openrouter`、`deepseek-official`
    pub id: String,
    pub display_name: String,
    /// 线缆协议：openai-completions / openai-responses / anthropic-messages；无法确定时为 None
    pub protocol: Option<String>,
    pub base_url: Option<String>,
    /// 凭据环境变量名（settings.yaml 的 apiKeyEnv，目录路由用内置目录缺省值）
    pub api_key_env: String,
    pub key_configured: bool,
    /// 脱敏 Key（未配置时为 None）
    pub masked_key: Option<String>,
}

/// 额度页数据源：DSH 配置根目录 + API 列表
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderList {
    /// DSH 数据根目录（供前端展示数据来源）
    pub home: String,
    pub providers: Vec<ProviderSummary>,
}

/// 结构化配置错误码，前端据此映射双语提示
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigErrorCode {
    /// DSH 配置不存在（settings.yaml 缺失，通常 DSH 未安装 / 未运行过）
    NotFound,
    /// 配置存在但无法读取 / 解析
    ParseFailed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigError {
    pub code: ConfigErrorCode,
    pub message: String,
}

/// DSH 用户数据根目录：`DSH_HOME` 环境变量 → 否则 `~/.dsh`
pub fn dsh_home() -> PathBuf {
    if let Some(h) = std::env::var_os("DSH_HOME") {
        let p = PathBuf::from(&h);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    home_dir().join(".dsh")
}

fn home_dir() -> PathBuf {
    // 不引入额外依赖：直接取标准用户目录环境变量（GUI 进程下 USERPROFILE/HOME 恒可用）
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
    }
}

/// 列出 DSH 配置中的全部 API。纯磁盘读取，不依赖 DSH 进程是否运行。
pub fn list_providers(dsh_home: &Path) -> Result<ProviderList, ConfigError> {
    let settings_path = dsh_home.join("settings.yaml");
    let text = std::fs::read_to_string(&settings_path).map_err(|e| {
        let code = if e.kind() == std::io::ErrorKind::NotFound {
            ConfigErrorCode::NotFound
        } else {
            ConfigErrorCode::ParseFailed
        };
        ConfigError {
            code,
            message: format!("{} 读取失败: {e}", settings_path.display()),
        }
    })?;

    let mut providers = providers_from_settings(&text)?;

    // Key 解析：进程环境变量 → .credentials.yaml refs；文件缺失/损坏视为无凭据（非错误）
    let refs = read_credentials_refs(dsh_home);
    for p in &mut providers {
        let key = resolve_key_with_refs(&refs, &p.api_key_env);
        p.key_configured = key.is_some();
        p.masked_key = key.as_deref().map(mask_key);
    }

    Ok(ProviderList {
        home: dsh_home.display().to_string(),
        providers,
    })
}

/// 解析 settings.yaml：固定路由 `deepseek-official` 置首，其余取 `llm-pi-ai.providers`。
fn providers_from_settings(yaml: &str) -> Result<Vec<ProviderSummary>, ConfigError> {
    let v: serde_yaml::Value = serde_yaml::from_str(yaml.trim_start_matches('\u{feff}')).map_err(|e| {
        ConfigError {
            code: ConfigErrorCode::ParseFailed,
            message: format!("settings.yaml 解析失败: {e}"),
        }
    })?;

    let mut out = vec![ProviderSummary {
        id: DEEPSEEK_OFFICIAL_ID.into(),
        display_name: "DeepSeek Official".into(),
        protocol: Some("openai-completions".into()),
        base_url: Some("https://api.deepseek.com".into()),
        api_key_env: "DEEPSEEK_API_KEY".into(),
        key_configured: false,
        masked_key: None,
    }];

    let Some(entries) = v
        .get("llm-pi-ai")
        .and_then(|s| s.get("providers"))
        .and_then(|p| p.as_mapping())
    else {
        return Ok(out);
    };

    for (k, entry) in entries {
        let Some(id) = k.as_str() else { continue };
        if id == DEEPSEEK_OFFICIAL_ID || entry.as_mapping().is_none() {
            continue;
        }
        let cat = catalog(id);
        let protocol = str_field(entry, "api").or_else(|| cat.map(|c| c.protocol.to_string()));
        let base_url = str_field(entry, "baseURL").or_else(|| cat.map(|c| c.base_url.to_string()));
        let api_key_env = str_field(entry, "apiKeyEnv")
            .unwrap_or_else(|| cat.map(|c| c.env.to_string()).unwrap_or_default());
        let display_name = str_field(entry, "displayName").unwrap_or_else(|| id.to_string());
        out.push(ProviderSummary {
            id: id.to_string(),
            display_name,
            protocol,
            base_url,
            api_key_env,
            key_configured: false,
            masked_key: None,
        });
    }
    Ok(out)
}

/// 取字符串字段：trim 后非空才算配置了
fn str_field(entry: &serde_yaml::Value, key: &str) -> Option<String> {
    entry
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 读取 `.credentials.yaml` 的 refs（环境变量名 → Key 明文）；缺失 / 损坏返回空表。
fn read_credentials_refs(dsh_home: &Path) -> HashMap<String, String> {
    match std::fs::read_to_string(dsh_home.join(".credentials.yaml")) {
        Ok(text) => parse_credentials_refs(&text).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

fn parse_credentials_refs(yaml: &str) -> Option<HashMap<String, String>> {
    let v: serde_yaml::Value = serde_yaml::from_str(yaml.trim_start_matches('\u{feff}')).ok()?;
    let mut out = HashMap::new();
    if let Some(m) = v.get("refs").and_then(|r| r.as_mapping()) {
        for (k, val) in m {
            if let (Some(k), Some(val)) = (k.as_str(), val.as_str()) {
                out.insert(k.to_string(), val.to_string());
            }
        }
    }
    Some(out)
}

/// 解析某 API 的 Key（进程环境变量 → 凭据文件 refs，与 DSH 自身优先级一致）。
pub fn resolve_key(dsh_home: &Path, api_key_env: &str) -> Option<String> {
    resolve_key_with_refs(&read_credentials_refs(dsh_home), api_key_env)
}

fn resolve_key_with_refs(refs: &HashMap<String, String>, api_key_env: &str) -> Option<String> {
    if !api_key_env.is_empty() {
        if let Ok(v) = std::env::var(api_key_env) {
            let t = v.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    refs.get(api_key_env)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 脱敏：长度 > 12 显示 `前6…后4`，否则原样（与旧版前端规则一致）。
pub fn mask_key(k: &str) -> String {
    let chars: Vec<char> = k.chars().collect();
    if chars.len() > 12 {
        let head: String = chars[..6].iter().collect();
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{head}…{tail}")
    } else {
        k.to_string()
    }
}

/// pi-ai 内置目录摘录：路由键 → (协议, 端点, 缺省凭据环境变量)。
/// 多协议路由（如 openrouter）取最常用的 openai-completions 作展示。
#[derive(Clone, Copy)]
struct CatalogRoute {
    protocol: &'static str,
    base_url: &'static str,
    env: &'static str,
}

fn catalog(route: &str) -> Option<CatalogRoute> {
    let (protocol, base_url, env) = match route {
        "openai" => ("openai-responses", "https://api.openai.com/v1", "OPENAI_API_KEY"),
        "openrouter" => ("openai-completions", "https://openrouter.ai/api/v1", "OPENROUTER_API_KEY"),
        "anthropic" => ("anthropic-messages", "https://api.anthropic.com", "ANTHROPIC_API_KEY"),
        "deepseek" => ("openai-completions", "https://api.deepseek.com", "DEEPSEEK_API_KEY"),
        "kimi-coding" => ("anthropic-messages", "https://api.kimi.com/coding", "KIMI_API_KEY"),
        "groq" => ("openai-completions", "https://api.groq.com/openai/v1", "GROQ_API_KEY"),
        "xai" => ("openai-responses", "https://api.x.ai/v1", "XAI_API_KEY"),
        "together" => ("openai-completions", "https://api.together.ai/v1", "TOGETHER_API_KEY"),
        "fireworks" => ("openai-completions", "https://api.fireworks.ai/inference", "FIREWORKS_API_KEY"),
        "moonshotai" => ("openai-completions", "https://api.moonshot.ai/v1", "MOONSHOT_API_KEY"),
        "moonshotai-cn" => ("openai-completions", "https://api.moonshot.cn/v1", "MOONSHOT_API_KEY"),
        "minimax" => ("anthropic-messages", "https://api.minimax.io/anthropic", "MINIMAX_API_KEY"),
        "minimax-cn" => ("anthropic-messages", "https://api.minimaxi.com/anthropic", "MINIMAX_CN_API_KEY"),
        "zai" => ("openai-completions", "https://api.z.ai/api/coding/paas/v4", "ZAI_API_KEY"),
        "zai-coding-cn" => (
            "openai-completions",
            "https://open.bigmodel.cn/api/coding/paas/v4",
            "ZAI_CODING_CN_API_KEY",
        ),
        "cerebras" => ("openai-completions", "https://api.cerebras.ai/v1", "CEREBRAS_API_KEY"),
        "nvidia" => ("openai-completions", "https://integrate.api.nvidia.com/v1", "NVIDIA_API_KEY"),
        "huggingface" => ("openai-completions", "https://router.huggingface.co/v1", "HF_TOKEN"),
        "baseten" => ("openai-completions", "https://inference.baseten.co/v1", "BASETEN_API_KEY"),
        "vercel-ai-gateway" => ("anthropic-messages", "https://ai-gateway.vercel.sh", "AI_GATEWAY_API_KEY"),
        _ => return None,
    };
    Some(CatalogRoute {
        protocol,
        base_url,
        env,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SETTINGS: &str = r#"
ui-onboarding:
  welcomeNoticeVersion: 2026-08-13.1
llm-pi-ai:
  providers:
    openrouter:
      models:
        - id: auto
          name: Auto
          contextWindow: 2000000
      apiKeyEnv: OPENROUTER_API_KEY
    kimi-coding:
      apiKeyEnv: KIMI_CODING_API_KEY
    acme-gateway:
      displayName: Acme Gateway
      apiKeyEnv: ACME_GATEWAY_API_KEY
      api: openai-completions
      baseURL: https://gateway.acme.example/v1
agent-default-model:
  provider: deepseek-official
  model: deepseek-flash
"#;

    #[test]
    fn providers_parse_with_catalog_fallback() {
        let list = providers_from_settings(SAMPLE_SETTINGS).unwrap();
        assert_eq!(list[0].id, "deepseek-official");
        assert_eq!(list[0].base_url.as_deref(), Some("https://api.deepseek.com"));
        assert_eq!(list[0].api_key_env, "DEEPSEEK_API_KEY");

        let openrouter = list.iter().find(|p| p.id == "openrouter").unwrap();
        // 目录路由：baseURL 未显式配置 → 从内置目录补全
        assert_eq!(openrouter.protocol.as_deref(), Some("openai-completions"));
        assert_eq!(openrouter.base_url.as_deref(), Some("https://openrouter.ai/api/v1"));
        assert_eq!(openrouter.api_key_env, "OPENROUTER_API_KEY");

        let kimi = list.iter().find(|p| p.id == "kimi-coding").unwrap();
        assert_eq!(kimi.protocol.as_deref(), Some("anthropic-messages"));
        assert_eq!(kimi.base_url.as_deref(), Some("https://api.kimi.com/coding"));
        assert_eq!(kimi.api_key_env, "KIMI_CODING_API_KEY");
    }

    #[test]
    fn explicit_fields_override_catalog() {
        let list = providers_from_settings(SAMPLE_SETTINGS).unwrap();
        let acme = list.iter().find(|p| p.id == "acme-gateway").unwrap();
        assert_eq!(acme.display_name, "Acme Gateway");
        assert_eq!(acme.protocol.as_deref(), Some("openai-completions"));
        assert_eq!(acme.base_url.as_deref(), Some("https://gateway.acme.example/v1"));
    }

    #[test]
    fn unknown_route_without_fields_has_no_protocol() {
        let yaml = "llm-pi-ai:\n  providers:\n    my-relay:\n      apiKeyEnv: MY_KEY\n";
        let list = providers_from_settings(yaml).unwrap();
        let p = &list[1];
        assert_eq!(p.id, "my-relay");
        assert!(p.protocol.is_none());
        assert!(p.base_url.is_none());
        assert_eq!(p.api_key_env, "MY_KEY");
    }

    #[test]
    fn bom_is_tolerated() {
        let list = providers_from_settings("\u{feff}llm-pi-ai:\n  providers: {}\n").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "deepseek-official");
    }

    #[test]
    fn missing_llm_pi_ai_yields_only_official() {
        let list = providers_from_settings("agent-default-model:\n  provider: deepseek-official\n").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "deepseek-official");
    }

    #[test]
    fn invalid_yaml_is_parse_failed() {
        let err = providers_from_settings("llm-pi-ai: [1, 2\n").unwrap_err();
        assert_eq!(err.code, ConfigErrorCode::ParseFailed);
    }

    #[test]
    fn credentials_refs_parse() {
        let refs = parse_credentials_refs(
            "version: 1\nrefs:\n  DEEPSEEK_API_KEY: sk-abc\n  EMPTY: \"\"\n",
        )
        .unwrap();
        assert_eq!(refs.get("DEEPSEEK_API_KEY").map(String::as_str), Some("sk-abc"));
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn resolve_prefers_env_then_refs() {
        let mut refs = HashMap::new();
        refs.insert("TEST_DSH_KEY_A".to_string(), "from-refs".to_string());
        // refs 兜底
        assert_eq!(
            resolve_key_with_refs(&refs, "TEST_DSH_KEY_A").as_deref(),
            Some("from-refs")
        );
        // 环境变量优先
        std::env::set_var("TEST_DSH_KEY_A", " from-env ");
        assert_eq!(
            resolve_key_with_refs(&refs, "TEST_DSH_KEY_A").as_deref(),
            Some("from-env")
        );
        std::env::remove_var("TEST_DSH_KEY_A");
    }

    #[test]
    fn resolve_skips_blank_values() {
        let mut refs = HashMap::new();
        refs.insert("TEST_DSH_KEY_B".to_string(), "   ".to_string());
        assert!(resolve_key_with_refs(&refs, "TEST_DSH_KEY_B").is_none());
        assert!(resolve_key_with_refs(&refs, "").is_none());
    }

    #[test]
    fn mask_key_shows_head_and_tail() {
        assert_eq!(mask_key("sk-0123456789abcdef"), "sk-012…cdef");
        assert_eq!(mask_key("short"), "short");
    }

    #[test]
    fn list_providers_end_to_end_resolves_keys() {
        let dir = std::env::temp_dir().join(format!("dsh-start-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.yaml"), SAMPLE_SETTINGS).unwrap();
        std::fs::write(
            dir.join(".credentials.yaml"),
            "version: 1\nrefs:\n  OPENROUTER_API_KEY: sk-or-test-0123456789abcdef\n",
        )
        .unwrap();

        let list = list_providers(&dir).unwrap();
        assert_eq!(list.home, dir.display().to_string());
        let openrouter = list.providers.iter().find(|p| p.id == "openrouter").unwrap();
        assert!(openrouter.key_configured);
        assert_eq!(openrouter.masked_key.as_deref(), Some("sk-or-…cdef"));
        // 未在 refs 中的凭据 → 未配置
        let acme = list.providers.iter().find(|p| p.id == "acme-gateway").unwrap();
        assert!(!acme.key_configured);
        assert!(acme.masked_key.is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn list_providers_missing_home_is_not_found() {
        let err = list_providers(Path::new("Z:/definitely/not/a/dsh-home")).unwrap_err();
        assert_eq!(err.code, ConfigErrorCode::NotFound);
    }
}
