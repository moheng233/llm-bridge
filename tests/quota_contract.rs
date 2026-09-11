use llm_bridge::{
    auth::quota::{QuotaError, adjust_usage, check_and_deduct},
    db::{
        self,
        models::{Token, UsageRecord, User, UserRole},
    },
};

async fn token(db: &db::Db, requests: i64, tokens: i64, period: &str) -> Token {
    let mut db = db.clone();
    let user = toasty::create!(User {
        oidc_sub: uuid::Uuid::new_v4().to_string(),
        name: "Quota contract",
        role: UserRole::Member,
        active: true,
    })
    .exec(&mut db)
    .await
    .unwrap();
    toasty::create!(Token {
        user_id: user.id,
        name: "quota-contract",
        token_hash: "not-used-by-quota-service",
        token_prefix: "lb_quota",
        allowed_models: "[]",
        request_quota: requests,
        token_quota: tokens,
        quota_period: period,
        active: true,
    })
    .exec(&mut db)
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_first_reservations_share_one_period_and_respect_limit() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", directory.path().join("quota.db").display());
    let first = db::init(db::all_models(), &url).await.unwrap();
    let second = db::init(db::all_models(), &url).await.unwrap();
    let token = token(&first, 5, 100, "monthly").await;
    let token_id = token.id;
    let start = std::sync::Arc::new(tokio::sync::Barrier::new(12));
    let mut tasks = Vec::new();
    for index in 0..12 {
        let db = if index % 2 == 0 {
            first.clone()
        } else {
            second.clone()
        };
        let start = start.clone();
        tasks.push(tokio::spawn(async move {
            let token = Token::get_by_id(&mut db.clone(), &token_id).await.unwrap();
            start.wait().await;
            match check_and_deduct(&db, token.id, 20).await {
                Ok(reservation) => {
                    adjust_usage(&db, &reservation, -5).await.unwrap();
                    true
                }
                Err(
                    QuotaError::RequestQuotaExceeded { .. } | QuotaError::TokenQuotaExceeded { .. },
                ) => false,
                Err(error) => panic!("database failure is not a quota rejection: {error}"),
            }
        }));
    }
    let mut admitted = 0;
    for task in tasks {
        admitted += usize::from(task.await.unwrap());
    }
    assert_eq!(admitted, 5);
    let records = UsageRecord::filter(UsageRecord::fields().token_id().eq(token_id))
        .exec(&mut first.clone())
        .await
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].request_count, 5);
    assert_eq!(records[0].token_count, 75);
}

#[tokio::test]
async fn rejected_reservation_leaves_no_usage_and_exact_boundary_is_admitted() {
    let db = db::init(db::all_models(), "sqlite::memory:").await.unwrap();
    let token = token(&db, 0, 100, "daily").await;
    assert!(matches!(
        check_and_deduct(&db, token.id, 101).await,
        Err(QuotaError::TokenQuotaExceeded { .. })
    ));
    let records = UsageRecord::all().exec(&mut db.clone()).await.unwrap();
    assert!(records.is_empty());
    let reservation = check_and_deduct(&db, token.id, 100).await.unwrap();
    assert!(matches!(
        check_and_deduct(&db, token.id, 1).await,
        Err(QuotaError::TokenQuotaExceeded { .. })
    ));
    adjust_usage(&db, &reservation, -10).await.unwrap();
    check_and_deduct(&db, token.id, 10).await.unwrap();
    let record = UsageRecord::get_by_id(&mut db.clone(), &reservation.record_id)
        .await
        .unwrap();
    assert_eq!((record.request_count, record.token_count), (2, 100));
}

#[tokio::test]
async fn settlement_targets_original_period_and_unlimited_usage_is_recorded() {
    let db = db::init(db::all_models(), "sqlite::memory:").await.unwrap();
    let token = token(&db, 0, 0, "unlimited").await;
    let mut reservation = check_and_deduct(&db, token.id, 20).await.unwrap();
    // 模拟已预留请求跨过周期边界；历史记录不会再被 current_period_key 选中。
    UsageRecord::filter(UsageRecord::fields().id().eq(reservation.record_id))
        .update()
        .period_key("2000-01")
        .exec(&mut db.clone())
        .await
        .unwrap();
    reservation.period_key = "2000-01".into();
    let next = check_and_deduct(&db, token.id, 10).await.unwrap();
    adjust_usage(&db, &reservation, 80).await.unwrap();
    let old = UsageRecord::get_by_id(&mut db.clone(), &reservation.record_id)
        .await
        .unwrap();
    let current = UsageRecord::get_by_id(&mut db.clone(), &next.record_id)
        .await
        .unwrap();
    assert_eq!((old.request_count, old.token_count), (1, 100));
    assert_eq!((current.request_count, current.token_count), (1, 10));
}
