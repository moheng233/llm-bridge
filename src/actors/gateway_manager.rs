use std::sync::Arc;

use keyring::{Entry, Error as KeyringError};
use ractor::{Actor, ActorProcessingErr, ActorRef};
use tracing::{Instrument, debug, info, info_span, instrument};

use crate::config::models::RuntimeSettings;
use crate::config::openrouter_catalog::{ModelCatalogSnapshot, OpenRouterCatalogClient};
use crate::db::{AvailableModel, DatabaseRepo, ResolvedProviderRoute};

pub struct GatewayManagerActor;

pub struct GatewayManagerArgs {
    pub settings: RuntimeSettings,
    pub database: Arc<DatabaseRepo>,
}

pub struct GatewayManagerState {
    pub settings: RuntimeSettings,
    pub database: Arc<DatabaseRepo>,
}

#[derive(Debug)]
pub enum GatewayManagerMessage {
    GetAvailableModels(ractor::RpcReplyPort<Result<Vec<AvailableModel>, String>>),
    ResolveModel(
        String,
        ractor::RpcReplyPort<Result<ResolvedProviderRoute, String>>,
    ),
    RefreshCatalog,
}

impl GatewayManagerMessage {
    fn kind(&self) -> &'static str {
        match self {
            Self::GetAvailableModels(_) => "get_available_models",
            Self::ResolveModel(_, _) => "resolve_model",
            Self::RefreshCatalog => "refresh_catalog",
        }
    }
}

#[ractor::async_trait]
impl Actor for GatewayManagerActor {
    type Msg = GatewayManagerMessage;
    type State = GatewayManagerState;
    type Arguments = GatewayManagerArgs;

    #[instrument(level = "info", skip(self, args))]
    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        info!("gateway manager starting");
        initialize_catalog(&args.database, &args.settings)
            .await
            .map_err(ActorProcessingErr::from)?;

        spawn_refresh_loop(myself, args.settings.model_catalog.refresh_interval_secs);
        info!("gateway manager initialized");

        Ok(GatewayManagerState {
            settings: args.settings,
            database: args.database,
        })
    }

    #[instrument(
        level = "debug",
        skip(self, state),
        fields(actor_id = ?_myself.get_id(), message = message.kind())
    )]
    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            GatewayManagerMessage::GetAvailableModels(reply) => {
                debug!("handling GetAvailableModels");
                let result = state
                    .database
                    .list_available_models()
                    .map_err(|error| error.to_string());
                let _ = reply.send(result);
            }
            GatewayManagerMessage::ResolveModel(model_name, reply) => {
                debug!(model = %model_name, "handling ResolveModel");
                let result = resolve_model_route(&state.database, &model_name).await;
                let _ = reply.send(result);
            }
            GatewayManagerMessage::RefreshCatalog => {
                debug!("handling RefreshCatalog");
                if let Err(error) = refresh_catalog(&state.database, &state.settings).await {
                    tracing::error!("failed to refresh model catalog: {}", error);
                }
            }
        }

        Ok(())
    }
}

fn spawn_refresh_loop(myself: ActorRef<GatewayManagerMessage>, interval_secs: u64) {
    info!(
        interval_secs = interval_secs.max(30),
        "catalog refresh loop started"
    );
    tokio::spawn(
        async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(interval_secs.max(30)));
            interval.tick().await;
            loop {
                interval.tick().await;
                if myself.cast(GatewayManagerMessage::RefreshCatalog).is_err() {
                    break;
                }
            }
        }
        .instrument(info_span!(
            "catalog_refresh_loop",
            interval_secs = interval_secs.max(30)
        )),
    );
}

#[instrument(level = "info", skip(database, settings), fields(strict_bootstrap = settings.model_catalog.strict_bootstrap))]
async fn initialize_catalog(
    database: &DatabaseRepo,
    settings: &RuntimeSettings,
) -> Result<(), String> {
    match refresh_catalog(database, settings).await {
        Ok(()) => Ok(()),
        Err(error) => {
            if settings.model_catalog.strict_bootstrap
                && database.catalog_model_count().unwrap_or(0) == 0
            {
                Err(error)
            } else {
                tracing::warn!("catalog bootstrap skipped: {}", error);
                Ok(())
            }
        }
    }
}

#[instrument(level = "info", skip(database, settings), fields(check_count = settings.model_catalog.count_consistency_check))]
async fn refresh_catalog(
    database: &DatabaseRepo,
    settings: &RuntimeSettings,
) -> Result<(), String> {
    info!("refreshing model catalog");
    let snapshot = fetch_model_catalog(settings).await?;

    if settings.model_catalog.strict_bootstrap && snapshot.len() == 0 {
        return Err("openrouter model catalog is empty".to_string());
    }

    if let Some(reported_count) = snapshot.reported_count {
        if reported_count != snapshot.fetched_count {
            tracing::warn!(
                "openrouter model count mismatch: count endpoint reports {}, list returned {}",
                reported_count,
                snapshot.fetched_count
            );
        }
    }

    info!(
        fetched_count = snapshot.fetched_count,
        reported_count = snapshot.reported_count,
        "persisting refreshed catalog snapshot"
    );

    database
        .replace_catalog(snapshot)
        .map_err(|error| format!("failed to persist model catalog: {error}"))
}

#[instrument(level = "info", skip(settings), fields(base_url = %settings.model_catalog.base_url))]
async fn fetch_model_catalog(settings: &RuntimeSettings) -> Result<ModelCatalogSnapshot, String> {
    let api_key = read_keyring_secret(
        &settings.model_catalog.api_key.service,
        &settings.model_catalog.api_key.account,
    )
    .map(Some)
    .unwrap_or_else(|error| {
        tracing::warn!(
            "openrouter catalog key not available for {}/{} ({}); using unauthenticated /models request",
            settings.model_catalog.api_key.service,
            settings.model_catalog.api_key.account,
            error
        );
        None
    });

    info!(
        authenticated = api_key.is_some(),
        "building openrouter catalog client"
    );

    let client = OpenRouterCatalogClient::new(&settings.model_catalog, api_key.as_deref())?;
    client.fetch_snapshot().await
}

#[instrument(level = "info", skip(database), fields(model = %model_name))]
async fn resolve_model_route(
    database: &DatabaseRepo,
    model_name: &str,
) -> Result<ResolvedProviderRoute, String> {
    let routes = database
        .resolve_model(model_name)
        .map_err(|error| format!("failed to resolve model {model_name}: {error}"))?;

    if routes.is_empty() {
        return Err(format!("no provider mapping found for model {model_name}"));
    }

    let mut last_error = None;

    for route in routes {
        match read_keyring_secret(&route.keyring_service, &route.keyring_account) {
            Ok(_) => {
                info!(
                    provider = %route.provider_name,
                    provider_model = %route.provider_model_name,
                    "resolved provider route"
                );
                return Ok(route);
            }
            Err(error) => {
                debug!(provider = %route.provider_name, error = %error, "provider credentials unavailable");
                last_error = Some(format!(
                    "provider {} credentials unavailable: {}",
                    route.provider_name, error
                ));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| format!("no usable provider found for model {model_name}")))
}

pub fn read_keyring_secret(service: &str, account: &str) -> Result<String, String> {
    let entry = Entry::new(service, account).map_err(|error| error.to_string())?;
    entry.get_password().map_err(map_keyring_error)
}

fn map_keyring_error(error: KeyringError) -> String {
    match error {
        KeyringError::NoEntry => "secret not found".to_string(),
        other => other.to_string(),
    }
}
