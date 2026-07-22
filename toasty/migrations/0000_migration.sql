CREATE TABLE "usage_records" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "token_id" INTEGER NOT NULL,
    "period_key" TEXT NOT NULL,
    "request_count" BIGINT NOT NULL,
    "token_count" BIGINT NOT NULL,
    "updated_at" TEXT NOT NULL
);
-- #[toasty::breakpoint]
CREATE INDEX "index_usage_records_by_token_id" ON "usage_records" ("token_id");
-- #[toasty::breakpoint]
CREATE TABLE "tokens" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "user_id" INTEGER NOT NULL,
    "name" TEXT NOT NULL,
    "token_hash" TEXT NOT NULL,
    "token_prefix" TEXT NOT NULL,
    "allowed_models" TEXT NOT NULL,
    "request_quota" BIGINT NOT NULL,
    "token_quota" BIGINT NOT NULL,
    "quota_period" TEXT NOT NULL,
    "active" BOOLEAN NOT NULL,
    "created_at" TEXT NOT NULL,
    "last_used_at" BIGINT
);
-- #[toasty::breakpoint]
CREATE INDEX "index_tokens_by_user_id" ON "tokens" ("user_id");
-- #[toasty::breakpoint]
CREATE TABLE "providers" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "provider_id" TEXT NOT NULL,
    "display_name" TEXT NOT NULL,
    "api_keys" TEXT NOT NULL,
    "enabled" BOOLEAN NOT NULL,
    "priority" BIGINT NOT NULL,
    "quota_adapter" TEXT CHECK ("quota_adapter" IN ('umans')),
    "quota_adapter_config" TEXT,
    "created_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL
);
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "index_providers_by_provider_id" ON "providers" ("provider_id");
-- #[toasty::breakpoint]
CREATE TABLE "provider_protocols" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "provider_id" INTEGER NOT NULL,
    "protocol" TEXT NOT NULL CHECK ("protocol" IN ('open_ai_chat_completions', 'open_ai_responses', 'anthropic_messages')),
    "base_url" TEXT NOT NULL,
    "compat_settings" TEXT,
    "enabled" BOOLEAN NOT NULL,
    "priority" BIGINT NOT NULL,
    "created_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL
);
-- #[toasty::breakpoint]
CREATE INDEX "index_provider_protocols_by_provider_id" ON "provider_protocols" ("provider_id");
-- #[toasty::breakpoint]
CREATE TABLE "users" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "oidc_sub" TEXT NOT NULL,
    "name" TEXT NOT NULL,
    "email" TEXT,
    "avatar_url" TEXT,
    "role" TEXT NOT NULL CHECK ("role" IN ('admin', 'member')),
    "active" BOOLEAN NOT NULL,
    "created_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL
);
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "index_users_by_oidc_sub" ON "users" ("oidc_sub");
-- #[toasty::breakpoint]
CREATE TABLE "models" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "model_name" TEXT NOT NULL,
    "display_name" TEXT NOT NULL,
    "description" TEXT,
    "max_input_tokens" BIGINT NOT NULL,
    "max_output_tokens" BIGINT NOT NULL,
    "tool_calling" BOOLEAN NOT NULL,
    "vision" BOOLEAN NOT NULL,
    "thinking" BOOLEAN NOT NULL,
    "adaptive_thinking" BOOLEAN NOT NULL,
    "status" TEXT,
    "created_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL
);
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "index_models_by_model_name" ON "models" ("model_name");
-- #[toasty::breakpoint]
CREATE TABLE "model_providers" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "model_id" INTEGER NOT NULL,
    "provider_id" INTEGER NOT NULL,
    "provider_model_id" TEXT NOT NULL,
    "protocol_id" INTEGER NOT NULL,
    "display_name" TEXT NOT NULL,
    "max_input_tokens" BIGINT,
    "max_output_tokens" BIGINT,
    "tool_calling" BOOLEAN,
    "vision" BOOLEAN,
    "thinking" BOOLEAN,
    "adaptive_thinking" BOOLEAN,
    "input_price_per_1m" REAL,
    "output_price_per_1m" REAL,
    "cache_read_price_per_1m" REAL,
    "enabled" BOOLEAN NOT NULL,
    "priority" BIGINT NOT NULL,
    "created_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL
);
-- #[toasty::breakpoint]
CREATE INDEX "index_model_providers_by_model_id" ON "model_providers" ("model_id");
-- #[toasty::breakpoint]
CREATE INDEX "index_model_providers_by_provider_id" ON "model_providers" ("provider_id");
-- #[toasty::breakpoint]
CREATE INDEX "index_model_providers_by_protocol_id" ON "model_providers" ("protocol_id");
