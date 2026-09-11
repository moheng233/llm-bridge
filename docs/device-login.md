# 设备码登录

网关提供 RFC 8628 风格流程，不是完整 OAuth Device Authorization 实现。插件无需本地回环服务器，不通过浏览器跳转直接领取 Token。

## 时序与接口

1. 插件 `POST /api/v1/auth/cli-sessions`，无 Session/Bearer 要求、无需请求体，200 返回：

   ```json
   {"sessionId":"cs_32位随机hex","userCode":"234567","verificationUrl":"https://bridge.example/auth/cli-verify?code=234567","expiresIn":600,"interval":5}
   ```

2. 打开 verificationUrl。`GET /auth/cli-verify` 是浏览器入口：需要登录时跳到 `/auth/login?next=...`，登录后返回验证页面；已登录则跳到 `/#/auth/cli-verify`。用户必须主动输入看到的六位码并点击确认。URL 的 code 仅作提示，不自动填写并授权。
3. 插件每 5 秒轮询 `GET /api/v1/auth/cli-sessions/{sessionId}`。sessionId 是高熵领取凭据，不是公开 ID；不得写日志、分享或作为浏览器 URL 参数。
4. 浏览器以 Session 调用 `POST /api/v1/auth/cli-sessions/confirm`：

   ```json
   {"userCode":"234567"}
   ```

   成功返回 200 `{"status":"approved"}`。确认不是返回明文 Token 的接口。
5. 插件下一次轮询得到 200：

   ```json
   {"status":"approved","token":"lb_...","tokenPrefix":"lb_..."}
   ```

   领取与 `approved → consumed` 在同一事务中，明文只返回一次并从会话中清除。客户端应立即保存至平台安全凭据存储；响应遗失后不能重新领取，需重新开始流程。

## 状态与错误

| 操作/状态 | HTTP | 响应 |
| --- | --- | --- |
| 轮询 pending | 202 | `{status:"pending"}`，`Retry-After: 5` |
| 首次领取 approved | 200 | status、token、tokenPrefix |
| consumed / expired | 410 | 对应 status，无 Token |
| 会话不存在 | 404 | error |
| 确认有效 pending | 200 | `{status:"approved"}` |
| 确认过期码 | 410 | error |
| 再次确认已确认/领取的码 | 409 | error |
| 无效/未知用户码 | 404 | error |
| 禁用账户确认 | 401 | error |

会话有效期 10 分钟，用户码为 2–9 构成的六位数字。轮询响应包含 `Cache-Control: no-store`；interval 是客户端应遵守的间隔，并非服务端强制限流声明。过期会话由轮询惰性处理，领取/过期清除暂存明文。

## 凭据与授权边界

确认事务按用户串行化：吊销该用户全部旧的有效 `vscode:` Token（保留行用于审计），签发一个新的 `vscode:<suffix>` Token，再批准会话。并发确认不同会话也不会留下多个有效 vscode Token；已被新授权取代的旧会话不能继续领取有效凭据。

既定默认不变：`allowedModels=[]` 表示全模型，requestQuota/tokenQuota=0、quotaPeriod=unlimited。它是个人 Token，不是团队强制预算。未配置 OIDC 或 Discovery 失败时，沿用免登录管理员模式；此时确认归于默认管理员，因此部署必须具备可信网络边界。

CliSession 为业务表，创建表不依赖删库；升级边界见 [architecture.md](architecture.md)。临时明文在 approved 至 consumed 期间存于数据库，数据库文件与备份需按凭据保护。上游真实 IdP 的端到端登录仍需部署环境验收，不以本地免登录 UI 结果替代。
