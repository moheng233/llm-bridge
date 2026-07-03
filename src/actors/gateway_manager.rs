use std::sync::Arc;

use ractor::{Actor, ActorProcessingErr, ActorRef};
use tracing::{Instrument, debug, info, info_span, instrument};

use crate::auth::quota;
use crate::config::models::RuntimeSettings;
use crate::db;
use crate::store::{self, Store};

pub struct GatewayManagerActor;

pub struct GatewayManagerArgs {
    pub settings: RuntimeSettings,
    pub store: Arc<Store>,
    pub db: db::Db,
}

pub struct GatewayManagerState {
    pub settings: RuntimeSettings,
    pub store: Arc<Store>,
    pub db: db::Db,
}

#[derive(Debug)]
pub enum GatewayManagerMessage {
    GetAvailableModels(ractor::RpcReplyPort<Result<Vec<store::AvailableModel>, String>>),
    ResolveModel(
        String,
        ractor::RpcReplyPort<Result<Vec<store::ResolvedProviderRoute>, String>>,
    ),
    ResetQuota,
}

impl GatewayManagerMessage {
    fn kind(&self) -> &'static str {
        match self {
            Self::GetAvailableModels(_) => "get_available_models",
            Self::ResolveModel(_, _) => "resolve_model",
            Self::ResetQuota => "reset_quota",
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
        spawn_quota_reset_loop(myself.clone());
        info!("gateway manager initialized");

        Ok(GatewayManagerState {
            settings: args.settings,
            store: args.store,
            db: args.db,
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
                match state.store.list_available_models().await {
                    Ok(models) => { let _ = reply.send(Ok(models)); }
                    Err(e) => { let _ = reply.send(Err(e)); }
                }
            }
            GatewayManagerMessage::ResolveModel(model_name, reply) => {
                debug!(model = %model_name, "handling ResolveModel");
                match state.store.resolve_model(&model_name).await {
                    Ok(routes) if !routes.is_empty() => { let _ = reply.send(Ok(routes)); }
                    Ok(_) => { let _ = reply.send(Err(format!("model '{}' is not available", model_name))); }
                    Err(e) => { let _ = reply.send(Err(e)); }
                }
            }
            GatewayManagerMessage::ResetQuota => {
                debug!("handling ResetQuota");
                if let Err(error) = quota::reset_expired_cycles(&state.db).await {
                    tracing::error!("failed to reset quota cycles: {}", error);
                }
            }
        }

        Ok(())
    }
}

fn spawn_quota_reset_loop(myself: ActorRef<GatewayManagerMessage>) {
    info!("quota reset loop started (hourly)");
    tokio::spawn(
        async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(3600));
            interval.tick().await;
            loop {
                interval.tick().await;
                if myself.cast(GatewayManagerMessage::ResetQuota).is_err() {
                    break;
                }
            }
        }
        .instrument(info_span!("quota_reset_loop")),
    );
}
