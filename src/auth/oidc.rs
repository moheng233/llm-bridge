//! OIDC 单点登录服务（Phase 1.4）。
//!
//! 实现 Authorization Code Flow：
//!   1. `discover` — 获取 Provider Metadata
//!   2. `login_url` — 生成 IdP 登录重定向 URL
//!   3. `callback` — 用 authorization code 交换 id_token，验证后返回用户信息
//!
//! OIDC 配置不存数据库，通过环境变量提供（见 [`OidcConfig`]）。

use openidconnect::core::{
    CoreAuthenticationFlow, CoreClient, CoreIdTokenClaims, CoreIdTokenVerifier,
    CoreProviderMetadata,
};
use openidconnect::reqwest;
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, RedirectUrl, Scope,
    TokenResponse,
};

use crate::config::models::OidcConfig;

/// OIDC 服务 — 封装与 IdP 的交互。
///
/// 不直接存储 [`CoreClient`]（因为其 typestate 泛型参数导致 struct 难以参数化），
/// 而是存储原始组件，每次需要时本地构建 client（零网络开销，仅类型封装）。
#[derive(Clone)]
pub struct OidcService {
    /// Provider metadata（含 auth/token/userinfo endpoint 和 JWKS）
    metadata: CoreProviderMetadata,
    client_id: ClientId,
    client_secret: Option<ClientSecret>,
    redirect_url: RedirectUrl,
    scopes: Vec<Scope>,
}

/// OIDC 回调后返回的用户信息（sub 用于匹配 `users.oidc_sub`）。
#[derive(Debug, Clone)]
pub struct OidcUser {
    pub sub: String,
    pub name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
}

impl OidcService {
    /// OIDC Discovery：启动时调用一次，失败拒绝启动（防 SSRF：不跟随重定向）。
    pub async fn discover(config: &OidcConfig) -> Result<Self, String> {
        let issuer_url = IssuerUrl::new(config.issuer_url.clone())
            .map_err(|e| format!("invalid issuer URL: {e}"))?;

        let http_client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none()) // 防 SSRF
            .build()
            .map_err(|e| format!("OIDC HTTP client: {e}"))?;

        let metadata = CoreProviderMetadata::discover_async(issuer_url, &http_client)
            .await
            .map_err(|e| format!("OIDC discovery failed: {e}"))?;

        let base_url = config.base_url.trim_end_matches('/');
        let redirect_url = RedirectUrl::new(format!("{base_url}/auth/callback"))
            .map_err(|e| format!("invalid redirect URL: {e}"))?;

        let client_id = ClientId::new(config.client_id.clone());
        let client_secret = (!config.client_secret.is_empty())
            .then(|| ClientSecret::new(config.client_secret.clone()));

        let scopes: Vec<Scope> = config
            .scopes
            .split_whitespace()
            .map(|s| Scope::new(s.to_string()))
            .collect();

        Ok(Self {
            metadata,
            client_id,
            client_secret,
            redirect_url,
            scopes,
        })
    }

    /// 生成 OIDC 登录 URL。返回 `(auth_url, csrf_token_secret, nonce_secret)`。
    ///
    /// 调用方需将 `csrf_token_secret` 和 `nonce_secret` 存入 Session，
    /// 然后 302 重定向用户浏览器到 `auth_url`。
    pub fn login_url(&self) -> (String, String, String) {
        let client = CoreClient::from_provider_metadata(
            self.metadata.clone(),
            self.client_id.clone(),
            self.client_secret.clone(),
        )
        .set_redirect_uri(self.redirect_url.clone());

        let mut req = client.authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        );

        for scope in &self.scopes {
            req = req.add_scope(scope.clone());
        }

        let (auth_url, csrf_token, nonce) = req.url();
        (
            auth_url.to_string(),
            csrf_token.secret().to_string(),
            nonce.secret().to_string(),
        )
    }

    /// OIDC 回调：用 authorization code 交换 id_token，验证后返回用户信息。
    pub async fn callback(&self, code: &str, nonce: &str) -> Result<OidcUser, String> {
        let client = CoreClient::from_provider_metadata(
            self.metadata.clone(),
            self.client_id.clone(),
            self.client_secret.clone(),
        )
        .set_redirect_uri(self.redirect_url.clone());

        let http_client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| format!("OIDC HTTP client: {e}"))?;

        let token_response = client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .map_err(|e| format!("token endpoint not configured: {e}"))?
            .request_async(&http_client)
            .await
            .map_err(|e| format!("token exchange failed: {e}"))?;

        let id_token = token_response
            .id_token()
            .ok_or_else(|| "no id_token in response".to_string())?;

        let id_token_verifier: CoreIdTokenVerifier = client.id_token_verifier();
        let nonce = Nonce::new(nonce.to_string());

        let claims: &CoreIdTokenClaims = id_token
            .claims(&id_token_verifier, &nonce)
            .map_err(|e| format!("id_token verification failed: {e}"))?;

        Ok(OidcUser {
            sub: claims.subject().as_str().to_string(),
            name: claims
                .name()
                .and_then(|n| n.get(None))
                .map(|n| n.as_str().to_string())
                .unwrap_or_default(),
            email: claims.email().map(|e| e.as_str().to_string()),
            avatar_url: claims
                .picture()
                .and_then(|p| p.get(None))
                .map(|p| p.as_str().to_string()),
        })
    }
}
