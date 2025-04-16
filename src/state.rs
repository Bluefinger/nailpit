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
use nailconfig::NailConfig;
use nailgen::MarkovGen;
use nailrng::FastRng;
use rand::seq::IndexedRandom;
use scc::HashMap;
use wyrand::RandomWyHashState;

use crate::peer::ProxiedPeer;

/// Smart pointer for all available Markov chains.
#[derive(Debug, Clone)]
pub struct NailInputs(Arc<[MarkovGen]>);

impl NailInputs {
    /// Pulls a random markov chain from the available list. Returns a cloned
    /// pointer to the selected chain.
    pub fn get_random_input(&self) -> MarkovGen {
        assert!(!self.0.is_empty());

        if self.0.len() == 1 {
            self.0[0].clone()
        } else {
            let mut rng = FastRng::default();

            self.0.choose(&mut rng).unwrap().clone()
        }
    }
}

impl From<Arc<[MarkovGen]>> for NailInputs {
    fn from(value: Arc<[MarkovGen]>) -> Self {
        Self(value)
    }
}

impl Deref for NailInputs {
    type Target = [MarkovGen];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct PeerState {
    visited: usize,
    pub last_seen: Instant,
}

type ShardedPeers = Arc<HashMap<IpAddr, PeerState, RandomWyHashState>>;

#[derive(Debug, Clone)]
pub struct PeerMap(ShardedPeers);

impl From<ShardedPeers> for PeerMap {
    #[inline]
    fn from(value: ShardedPeers) -> Self {
        Self(value)
    }
}

impl Deref for PeerMap {
    type Target = ShardedPeers;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PeerMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig(Arc<NailConfig>);

impl AppConfig {
    pub fn clone_inner(&self) -> Arc<NailConfig> {
        self.0.clone()
    }
}

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
    pub sources: PeerMap,
    pub config: AppConfig,
    pub inputs: NailInputs,
}

impl ServerState {
    pub fn new(
        sources: impl Into<PeerMap>,
        config: impl Into<AppConfig>,
        inputs: impl Into<NailInputs>,
    ) -> Self {
        Self {
            sources: sources.into(),
            config: config.into(),
            inputs: inputs.into(),
        }
    }
}

impl FromRef<ServerState> for PeerMap {
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

impl FromRef<ServerState> for NailInputs {
    #[inline]
    fn from_ref(input: &ServerState) -> Self {
        input.inputs.clone()
    }
}

impl<S> FromRequestParts<S> for PeerMap
where
    PeerMap: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Infallible;

    #[fastrace::trace]
    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(PeerMap::from_ref(state))
    }
}

impl<S> FromRequestParts<S> for AppConfig
where
    AppConfig: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Infallible;

    #[fastrace::trace]
    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(AppConfig::from_ref(state))
    }
}

impl<S> FromRequestParts<S> for NailInputs
where
    NailInputs: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Infallible;

    #[fastrace::trace]
    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(NailInputs::from_ref(state))
    }
}

#[fastrace::trace]
pub async fn track_incoming_sources(
    sources: PeerMap,
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
        .or_insert_with(|| PeerState {
            visited: 1,
            last_seen: Instant::now(),
        });

    log::info!("Saw: {} at {:?}", ip, seen.last_seen);

    Ok(next.run(request).await)
}
