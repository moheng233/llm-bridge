use std::collections::HashMap;
use std::sync::Arc;

use ractor::{Actor, ActorProcessingErr, ActorRef};

use crate::config::models::{AppConfig, ProviderConfig};
use crate::config::openrouter_catalog::{ModelCatalogSnapshot, OpenRouterCatalogClient};
use crate::routing::models::RouteGroup;

pub struct GatewayManagerActor;

pub struct GatewayManagerState {
    pub route_groups: HashMap<String, Arc<RouteGroup>>,
    pub model_catalog: ModelCatalogSnapshot,
    pub config: AppConfig,
}

#[derive(Debug)]
pub enum GatewayManagerMessage {
    GetRouteGroups(ractor::RpcReplyPort<Vec<Arc<RouteGroup>>>),
    GetRouteGroup(String, ractor::RpcReplyPort<Option<Arc<RouteGroup>>>),
    GetProviderConfig(String, ractor::RpcReplyPort<Option<ProviderConfig>>),
    RefreshModelCatalog,
    ReloadConfig(AppConfig),
}

#[ractor::async_trait]
impl Actor for GatewayManagerActor {
    type Msg = GatewayManagerMessage;
    type State = GatewayManagerState;
    type Arguments = AppConfig;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let model_catalog = fetch_model_catalog(&args)
            .await
            .map_err(ActorProcessingErr::from)?;
        let route_groups = args
            .to_route_groups_with_catalog(&model_catalog)
            .map_err(|error| ActorProcessingErr::from(error.to_string()))?;
        let map = build_route_group_map(route_groups);

        spawn_refresh_loop(myself.clone(), args.model_catalog.refresh_interval_secs);

        Ok(GatewayManagerState {
            route_groups: map,
            model_catalog,
            config: args,
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            GatewayManagerMessage::GetRouteGroups(reply) => {
                let groups: Vec<Arc<RouteGroup>> = state.route_groups.values().cloned().collect();
                let _ = reply.send(groups);
            }
            GatewayManagerMessage::GetRouteGroup(id, reply) => {
                let group = state.route_groups.get(&id).cloned();
                let _ = reply.send(group);
            }
            GatewayManagerMessage::GetProviderConfig(id, reply) => {
                let provider = state
                    .config
                    .providers
                    .iter()
                    .find(|provider| provider.id == id)
                    .cloned();
                let _ = reply.send(provider);
            }
            GatewayManagerMessage::RefreshModelCatalog => {
                match fetch_model_catalog(&state.config).await {
                    Ok(model_catalog) => {
                        if let Err(error) = refresh_route_groups(state, model_catalog) {
                            tracing::error!(
                                "failed to refresh route groups from model catalog: {}",
                                error
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!("failed to refresh model catalog: {}", error);
                    }
                }
            }
            GatewayManagerMessage::ReloadConfig(new_config) => {
                match fetch_model_catalog(&new_config).await {
                    Ok(model_catalog) => {
                        match new_config.to_route_groups_with_catalog(&model_catalog) {
                            Ok(route_groups) => {
                                state.route_groups = build_route_group_map(route_groups);
                                state.model_catalog = model_catalog;
                                state.config = new_config;
                                tracing::info!("config reloaded successfully");
                            }
                            Err(error) => {
                                tracing::error!(
                                    "failed to rebuild route groups for new config: {}",
                                    error
                                );
                            }
                        }
                    }
                    Err(error) => {
                        tracing::error!(
                            "failed to reload config because model catalog load failed: {}",
                            error
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

fn build_route_group_map(route_groups: Vec<RouteGroup>) -> HashMap<String, Arc<RouteGroup>> {
    let mut map = HashMap::new();
    for group in route_groups {
        map.insert(group.id.clone(), Arc::new(group));
    }
    map
}

fn spawn_refresh_loop(myself: ActorRef<GatewayManagerMessage>, interval_secs: u64) {
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(interval_secs.max(30)));
        interval.tick().await;
        loop {
            interval.tick().await;
            if myself
                .cast(GatewayManagerMessage::RefreshModelCatalog)
                .is_err()
            {
                break;
            }
        }
    });
}

async fn fetch_model_catalog(config: &AppConfig) -> Result<ModelCatalogSnapshot, String> {
    let source_provider = config
        .providers
        .iter()
        .find(|provider| provider.id == config.model_catalog.source_provider_id)
        .ok_or_else(|| {
            format!(
                "model catalog source provider {} not found",
                config.model_catalog.source_provider_id
            )
        })?;

    let client = OpenRouterCatalogClient::new(&config.model_catalog, &source_provider.api_key)?;
    let snapshot = client.fetch_snapshot().await?;

    if let Some(reported_count) = snapshot.reported_count {
        if reported_count != snapshot.fetched_count {
            tracing::warn!(
                "openrouter model count mismatch: count endpoint reports {}, list returned {}",
                reported_count,
                snapshot.fetched_count
            );
        }
    }

    if config.model_catalog.strict_bootstrap && snapshot.len() == 0 {
        return Err("openrouter model catalog is empty".to_string());
    }

    Ok(snapshot)
}

fn refresh_route_groups(
    state: &mut GatewayManagerState,
    model_catalog: ModelCatalogSnapshot,
) -> Result<(), String> {
    let route_groups = state
        .config
        .to_route_groups_with_catalog(&model_catalog)
        .map_err(|error| format!("route group rebuild failed: {error}"))?;

    state.route_groups = build_route_group_map(route_groups);
    state.model_catalog = model_catalog;
    tracing::info!("model catalog refreshed successfully");
    Ok(())
}
