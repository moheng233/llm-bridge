use std::sync::Arc;

use ractor::{Actor, ActorProcessingErr, ActorRef};
use tracing::{Instrument, debug, info, info_span, instrument};

use crate::config::models::RuntimeSettings;
use crate::config::models_dev_catalog::ModelsDevCatalogClient;
use crate::store::{AvailableModel, ResolvedProviderRoute, Store};

pub struct GatewayManagerActor;

pub struct GatewayManagerArgs {
    pub settings: RuntimeSettings,
    pub store: Arc<Store>,
}

pub struct GatewayManagerState {
    pub settings: RuntimeSettings,
    pub store: Arc<Store>,
}

#[derive(Debug)]
pub enum GatewayManagerMessage {
    GetAvailableModels(ractor::RpcReplyPort<Result<Vec<AvailableModel>, String>>),
    ResolveModel(
        String,
        ractor::RpcReplyPort<Result<Vec<ResolvedProviderRoute>, String>>,
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
        initialize_catalog(&args.store, &args.settings)
            .await
            .map_err(ActorProcessingErr::from)?;

        spawn_refresh_loop(myself, args.settings.model_catalog.refresh_interval_secs);
        info!("gateway manager initialized");

        Ok(GatewayManagerState {
            settings: args.settings,
            store: args.store,
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
                let models = state.store.list_available_models();
                let _ = reply.send(Ok(models));
            }
            GatewayManagerMessage::ResolveModel(model_name, reply) => {
                debug!(model = %model_name, "handling ResolveModel");
                let routes = state.store.resolve_model(&model_name);
                if routes.is_empty() {
                    let _ = reply.send(Err(format!("model '{}' is not available", model_name)));
                } else {
                    let _ = reply.send(Ok(routes));
                }
            }
            GatewayManagerMessage::RefreshCatalog => {
                debug!("handling RefreshCatalog");
                if let Err(error) = refresh_catalog(&state.store, &state.settings).await {
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

#[instrument(level = "info", skip(store, settings), fields(strict_bootstrap = settings.model_catalog.strict_bootstrap))]
async fn initialize_catalog(
    store: &Store,
    settings: &RuntimeSettings,
) -> Result<(), String> {
    // Try loading from local cache first.
    match ModelsDevCatalogClient::load_cache(settings.store_path.as_ref()) {
        Ok(Some((data, metadata))) => {
            info!(
                fetched_at = metadata.fetched_at,
                "catalog loaded from local cache"
            );
            store
                .replace_catalog(data, metadata)
                .map_err(|e| e.to_string())?;
        }
        Ok(None) => {
            info!("no local cache found, fetching from models.dev");
            do_fetch_and_store(store, settings).await?;
        }
        Err(e) => {
            tracing::warn!("failed to load cache: {}, fetching from models.dev", e);
            do_fetch_and_store(store, settings).await?;
        }
    }

    Ok(())
}

async fn do_fetch_and_store(
    store: &Store,
    settings: &RuntimeSettings,
) -> Result<(), String> {
    let client = ModelsDevCatalogClient::new(&settings.model_catalog)?;
    match client.fetch(None).await {
        Ok((data, metadata)) => {
            store
                .replace_catalog(data, metadata)
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        Err(e) if e == "unchanged" => Ok(()),
        Err(e) => {
            // If strict_bootstrap and store is empty, fail.
            if settings.model_catalog.strict_bootstrap && store.catalog_model_count() == 0 {
                Err(format!("catalog bootstrap failed and store is empty: {e}"))
            } else {
                tracing::warn!("catalog refresh failed: {}", e);
                Ok(())
            }
        }
    }
}

#[instrument(level = "info", skip(store, settings))]
async fn refresh_catalog(
    store: &Store,
    settings: &RuntimeSettings,
) -> Result<(), String> {
    info!("refreshing model catalog from models.dev");
    let client = ModelsDevCatalogClient::new(&settings.model_catalog)?;

    // Use the stored etag for conditional requests.
    let metadata = store.get_metadata();
    let (_data, metadata) = match client.fetch(metadata.etag.as_deref()).await {
        Ok(result) => result,
        Err(e) if e == "unchanged" => {
            info!("catalog unchanged, skipping refresh");
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    // After a successful fetch, we need to properly write the cache.
    // Re-fetch without etag to get the full data for caching.
    let (data, metadata) = client.fetch(None).await?;
    store
        .replace_catalog(data, metadata)
        .map_err(|e| e.to_string())
}
