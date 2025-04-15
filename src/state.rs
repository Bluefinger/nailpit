use std::{
    convert::Infallible,
    net::IpAddr,
    ops::{Deref, DerefMut},
    sync::Arc,
    time::Instant,
};

use axum::{
    extract::{FromRef, FromRequestParts, Request},
    middleware::Next,
    response::Response,
};
use hyper::StatusCode;
use scc::HashMap;
use wyrand::RandomWyHashState;

use crate::{config::NailConfig, peer::ProxiedPeer};

#[derive(Debug, Clone)]
pub struct SourceState {
    visited: usize,
    pub last_seen: Instant,
}

type ShardedSources = Arc<HashMap<IpAddr, SourceState, RandomWyHashState>>;

#[derive(Debug, Clone)]
pub struct SourceMap(ShardedSources);

impl From<ShardedSources> for SourceMap {
    #[inline]
    fn from(value: ShardedSources) -> Self {
        Self(value)
    }
}

impl Deref for SourceMap {
    type Target = ShardedSources;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SourceMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig(Arc<NailConfig>);

impl From<Arc<NailConfig>> for AppConfig {
    #[inline]
    fn from(value: Arc<NailConfig>) -> Self {
        Self(value)
    }
}

impl Deref for AppConfig {
    type Target = NailConfig;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct ServerState {
    pub sources: SourceMap,
    pub config: AppConfig,
}

impl ServerState {
    pub fn new(sources: impl Into<SourceMap>, config: impl Into<AppConfig>) -> Self {
        Self {
            sources: sources.into(),
            config: config.into(),
        }
    }
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new(
            Arc::new(HashMap::with_capacity_and_hasher(
                128,
                RandomWyHashState::new(),
            )),
            Arc::new(Default::default()),
        )
    }
}

impl FromRef<ServerState> for SourceMap {
    #[inline]
    fn from_ref(input: &ServerState) -> Self {
        input.sources.clone()
    }
}

impl FromRef<ServerState> for AppConfig {
    #[inline]
    fn from_ref(input: &ServerState) -> Self {
        input.config.clone()
    }
}

impl<S> FromRequestParts<S> for SourceMap
where
    SourceMap: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Infallible;

    #[fastrace::trace]
    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(SourceMap::from_ref(state))
    }
}

impl<S> FromRequestParts<S> for AppConfig
where
    AppConfig: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(AppConfig::from_ref(state))
    }
}

#[fastrace::trace]
pub async fn track_incoming_sources(
    sources: SourceMap,
    proxied: ProxiedPeer,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let ip = proxied.ip();

    let seen = sources
        .entry_async(ip)
        .await
        .and_modify(|source| {
            source.visited += 1;
            source.last_seen = Instant::now()
        })
        .or_insert_with(|| SourceState {
            visited: 1,
            last_seen: Instant::now(),
        });

    log::info!("Saw: {} at {:?}", ip, seen.last_seen);

    Ok(next.run(request).await)
}
