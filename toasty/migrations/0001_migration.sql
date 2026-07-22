CREATE TABLE "usage_daily" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "day" TEXT NOT NULL,
    "token_id" INTEGER NOT NULL,
    "model" TEXT NOT NULL,
    "request_count" BIGINT NOT NULL,
    "input_tokens" BIGINT NOT NULL,
    "output_tokens" BIGINT NOT NULL,
    "reasoning_tokens" BIGINT NOT NULL,
    "cached_tokens" BIGINT NOT NULL,
    "total_tokens" BIGINT NOT NULL,
    "cost_usd" REAL NOT NULL,
    "updated_at" TEXT NOT NULL
);
-- #[toasty::breakpoint]
CREATE INDEX "index_usage_daily_by_day" ON "usage_daily" ("day");
-- #[toasty::breakpoint]
CREATE INDEX "index_usage_daily_by_token_id" ON "usage_daily" ("token_id");
-- #[toasty::breakpoint]
CREATE INDEX "index_usage_daily_by_model" ON "usage_daily" ("model");
-- #[toasty::breakpoint]
CREATE TABLE "llm_request_traces" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "request_id" TEXT NOT NULL,
    "trace_id" TEXT,
    "interface" TEXT NOT NULL CHECK ("interface" IN ('open_ai_http', 'ws_rpc')),
    "token_id" INTEGER NOT NULL,
    "user_id" INTEGER NOT NULL,
    "token_prefix" TEXT NOT NULL,
    "model" TEXT NOT NULL,
    "provider_id" TEXT NOT NULL,
    "provider_model_id" TEXT NOT NULL,
    "protocol" TEXT NOT NULL,
    "status" TEXT NOT NULL CHECK ("status" IN ('pending', 'streaming', 'success', 'error', 'cancelled')),
    "error_type" TEXT,
    "error_message" TEXT,
    "upstream_status" INTEGER,
    "finish_reason" TEXT,
    "estimated_tokens" BIGINT NOT NULL,
    "input_tokens" INTEGER,
    "output_tokens" INTEGER,
    "reasoning_tokens" INTEGER,
    "cached_tokens" INTEGER,
    "total_tokens" INTEGER,
    "cost_usd" REAL,
    "upstream_request_id" TEXT,
    "created_at" TEXT NOT NULL,
    "first_chunk_at" TEXT,
    "completed_at" TEXT,
    "ttft_ms" BIGINT,
    "latency_ms" BIGINT,
    "request_messages" TEXT,
    "response_parts" TEXT
);
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "index_llm_request_traces_by_request_id" ON "llm_request_traces" ("request_id");
-- #[toasty::breakpoint]
CREATE INDEX "index_llm_request_traces_by_token_id" ON "llm_request_traces" ("token_id");
-- #[toasty::breakpoint]
CREATE INDEX "index_llm_request_traces_by_user_id" ON "llm_request_traces" ("user_id");
-- #[toasty::breakpoint]
CREATE INDEX "index_llm_request_traces_by_model" ON "llm_request_traces" ("model");
