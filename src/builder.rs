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
    indexer1::{Indexer, Processor},
    storage::LogStorage,
};

pub struct IndexerBuilder<S: LogStorage, P: Processor<S::Transaction>> {
    http_provider: Option<Url>,
    ws_provider: Option<Url>,
    poll_interval: Option<Duration>,
    filter: Option<Filter>,
    processor: Option<P>,
    storage: Option<S>,
    block_range_limit: Option<u64>,
}

impl<S: LogStorage, P: Processor<S::Transaction>> Default for IndexerBuilder<S, P> {
    fn default() -> Self {
        Self {
            http_provider: None,
            ws_provider: None,
            poll_interval: None,
            filter: None,
            processor: None,
            storage: None,
            block_range_limit: None,
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
        self.http_provider = Some(url);
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

    pub fn ws_rpc_url(mut self, url: Url) -> Self {
        self.ws_provider = Some(url);
        self
    }

    pub fn ws_rpc_url_opt(mut self, url: Option<Url>) -> Self {
        self.ws_provider = url;
        self
    }

    pub fn fetch_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = Some(interval);
        self
    }

    pub async fn build(self) -> anyhow::Result<Indexer<S, P>> {
        let http_url = self
            .http_provider
            .ok_or(anyhow!("Http porvider is missing"))?;

        let http_provider = ProviderBuilder::new().connect_http(http_url);

        let ws_provider: Option<Box<dyn Provider>> = match self.ws_provider {
            Some(url) => Some(Box::new(
                ProviderBuilder::new()
                    .connect_ws(WsConnect::new(url.to_string()))
                    .await
                    .with_context(|| anyhow!("Failed to connect to rpc via WS"))?,
            )),
            None => None,
        };

        let processor = self.processor.ok_or(anyhow!("Processor is missing"))?;
        let filter = self.filter.ok_or(anyhow!("Filter is missing"))?;
        let fetch_interval = self
            .poll_interval
            .ok_or(anyhow!("Fetch interval is missing"))?;

        let storage = self.storage.ok_or(anyhow!("Storage is missing"))?;

        Indexer::new(
            processor,
            filter,
            Box::new(http_provider),
            ws_provider,
            fetch_interval,
            storage,
            self.block_range_limit,
        )
        .await
    }
}
