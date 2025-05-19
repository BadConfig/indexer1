//! Indexer building tools
use std::time::Duration;

use alloy::{
    providers::{Provider, ProviderBuilder, WsConnect},
    rpc::types::Filter,
    transports::http::reqwest::Url,
};
use anyhow::{anyhow, Context};
use sqlx::{PgPool, Postgres, Sqlite, SqlitePool};

use crate::{
    indexer1::{FinalityLevel, Indexer, Processor},
    storage::LogStorage,
};

pub struct IndexerBuilder<S: LogStorage, P: Processor<S::Transaction>> {
    http_provider_url: Option<Url>,
    http_provider_client: Option<Box<dyn Provider>>,
    ws_provider_client: Option<Box<dyn Provider>>,
    ws_provider_url: Option<Url>,
    fetch_interval: Option<Duration>,
    overtake_interval: Option<Duration>,
    filter: Option<Filter>,
    processor: Option<P>,
    storage: Option<S>,
    block_range_limit: Option<u64>,
    finality_level: FinalityLevel,
}

impl<S: LogStorage, P: Processor<S::Transaction>> Default for IndexerBuilder<S, P> {
    fn default() -> Self {
        Self {
            http_provider_client: None,
            http_provider_url: None,
            ws_provider_client: None,
            ws_provider_url: None,
            fetch_interval: None,
            overtake_interval: None,
            filter: None,
            processor: None,
            storage: None,
            block_range_limit: None,
            finality_level: FinalityLevel::Finalized,
        }
    }
}

impl<P: Processor<sqlx::Transaction<'static, Postgres>>> IndexerBuilder<PgPool, P> {
    pub fn pg_storage(mut self, pool: PgPool) -> Self {
        self.storage = Some(pool);
        self
    }
}

impl<P: Processor<sqlx::Transaction<'static, Sqlite>>> IndexerBuilder<SqlitePool, P> {
    pub fn sqlite_storage(mut self, pool: SqlitePool) -> Self {
        self.storage = Some(pool);
        self
    }
}

impl<S: LogStorage, P: Processor<S::Transaction>> IndexerBuilder<S, P> {
    pub fn http_rpc_url(mut self, url: Url) -> Self {
        self.http_provider_url = Some(url);
        self
    }

    pub fn http_provider(mut self, p: Box<dyn Provider>) -> Self {
        self.http_provider_client = Some(p);
        self
    }

    pub fn filter(mut self, filter_data: Filter) -> Self {
        self.filter = Some(filter_data);
        self
    }

    pub fn set_processor(mut self, function: P) -> Self {
        self.processor = Some(function);
        self
    }

    pub fn block_range_limit(mut self, limit: u64) -> Self {
        self.block_range_limit = Some(limit);
        self
    }

    pub fn block_range_limit_opt(mut self, limit: Option<u64>) -> Self {
        self.block_range_limit = limit;
        self
    }

    pub fn ws_provider(mut self, p: Box<dyn Provider>) -> Self {
        self.ws_provider_client = Some(p);
        self
    }

    pub fn ws_provider_opt(mut self, p: Option<Box<dyn Provider>>) -> Self {
        self.ws_provider_client = p;
        self
    }

    pub fn ws_rpc_url(mut self, url: Url) -> Self {
        self.ws_provider_url = Some(url);
        self
    }

    pub fn ws_rpc_url_opt(mut self, url: Option<Url>) -> Self {
        self.ws_provider_url = url;
        self
    }

    pub fn overtake_interval(mut self, interval: Duration) -> Self {
        self.overtake_interval = Some(interval);
        self
    }

    pub fn fetch_interval(mut self, interval: Duration) -> Self {
        self.fetch_interval = Some(interval);
        self
    }

    pub fn finality_level(mut self, level: FinalityLevel) -> Self {
        self.finality_level = level;
        self
    }

    //TODO: providers can be generic types not dyns
    pub async fn build(self) -> anyhow::Result<Indexer<S, P>> {
        let http_provider = match self.http_provider_client {
            Some(p) => p,
            None => {
                let http_url = self
                    .http_provider_url
                    .ok_or(anyhow!("Http porvider is missing"))?;
                Box::new(ProviderBuilder::new().connect_http(http_url))
            }
        };

        let ws_provider: Option<Box<dyn Provider>> = match self.ws_provider_client {
            Some(p) => Some(p),
            None => match self.ws_provider_url {
                Some(url) => Some(Box::new(
                    ProviderBuilder::new()
                        .connect_ws(WsConnect::new(url.to_string()))
                        .await
                        .with_context(|| anyhow!("Failed to connect to rpc via WS"))?,
                )),
                None => None,
            },
        };

        let log_processor = self.processor.ok_or(anyhow!("Processor is missing"))?;
        let filter = self.filter.ok_or(anyhow!("Filter is missing"))?;
        let fetch_interval = self
            .fetch_interval
            .ok_or(anyhow!("Fetch interval is missing"))?;

        let storage = self.storage.ok_or(anyhow!("Storage is missing"))?;

        let chain_id = http_provider.get_chain_id().await?;

        let (last_observed_block, filter_id) =
            storage.get_or_create_filter(&filter, chain_id).await?;

        Ok(Indexer {
            log_processor,
            filter,
            storage,
            chain_id,
            filter_id,
            last_observed_block,
            http_provider,
            ws_provider,
            fetch_interval,
            overtake_interval: self.overtake_interval.unwrap_or(fetch_interval),
            block_range_limit: self.block_range_limit,
            finality_level: self.finality_level.into(),
        })
    }
}
