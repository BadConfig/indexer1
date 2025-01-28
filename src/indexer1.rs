//! Indexer implementation
use std::time::Duration;

use alloy::{
    primitives::keccak256,
    providers::{Provider, RootProvider},
    pubsub::PubSubFrontend,
    rpc::types::{Filter, Log},
    sol_types::SolValue,
    transports::http::{Client, Http},
};
use futures::{stream, Future, Stream};
use sha2::{Digest, Sha256};
use sqlx::{Database, Transaction};
use tokio::time::interval;
use tokio_stream::{wrappers::IntervalStream, StreamExt as StreamExtTokio};
use tokio_util::sync::CancellationToken;

use crate::{builder::IndexerBuilder, storage::LogStorage};

pub trait Processor {
    /// Function is invoked every time new logs are found. They are guaranteed to be ordered and
    /// not duplicated if applied to database through given transaction.
    fn process<DB: Database>(
        &mut self,
        logs: &[Log],
        transaction: &mut Transaction<'static, DB>,
        chain_id: u64,
    ) -> impl Future<Output = anyhow::Result<()>>;
}

pub fn filter_id(filter: &Filter, chain_id: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(chain_id.abi_encode());
    hasher.update(filter.get_from_block().unwrap_or(1).abi_encode());

    let mut topics: Vec<_> = filter.topics[0].iter().collect();
    topics.sort();
    for topic in topics {
        hasher.update(topic.abi_encode());
    }

    let mut topics: Vec<_> = filter.topics[1].iter().collect();
    topics.sort();
    for topic in topics {
        hasher.update(topic.abi_encode());
    }

    let mut topics: Vec<_> = filter.topics[2].iter().collect();
    topics.sort();
    for topic in topics {
        hasher.update(topic.abi_encode());
    }

    let mut topics: Vec<_> = filter.topics[3].iter().collect();
    topics.sort();
    for topic in topics {
        hasher.update(topic.abi_encode());
    }

    let mut addresses: Vec<_> = filter.address.iter().collect();
    addresses.sort();
    for address in addresses {
        hasher.update(address.abi_encode());
    }

    let result = hasher.finalize();
    keccak256(result).to_string()
}

pub struct Indexer<P: Processor, S: LogStorage> {
    chain_id: u64,
    filter_id: String,
    filter: Filter,
    last_observed_block: u64,
    storage: S,
    log_processor: P,
    provider: RootProvider<Http<Client>>,
    ws_provider: Option<RootProvider<PubSubFrontend>>,
    fetch_interval: Duration,
}

impl<P: Processor, S: LogStorage> Indexer<P, S> {
    pub fn builder() -> IndexerBuilder<P, S> {
        IndexerBuilder::default()
    }

    pub async fn new(
        log_processor: P,
        filter: Filter,
        provider: RootProvider<Http<Client>>,
        ws_provider: Option<RootProvider<PubSubFrontend>>,
        fetch_interval: Duration,
        storage: S,
    ) -> anyhow::Result<Self> {
        let chain_id = provider.get_chain_id().await?;

        let (last_observed_block, filter_id) =
            storage.get_or_create_filter(&filter, chain_id).await?;

        Ok(Self {
            log_processor,
            filter,
            storage,
            chain_id,
            filter_id,
            last_observed_block,
            provider,
            ws_provider,
            fetch_interval,
        })
    }

    pub async fn run(mut self, cancel_token: CancellationToken) -> anyhow::Result<()> {
        let poll_interval = IntervalStream::new(interval(self.fetch_interval));
        let ws_watcher = self.spawn_ws_watcher().await?;
        tokio::pin!(ws_watcher);
        tokio::pin!(poll_interval);

        log::info!("Succesfully initialised watcher and poller");
        loop {
            tokio::select! {
                Some(_) = ws_watcher.next() => {}
                Some(_) = poll_interval.next() => {}
                _ = cancel_token.cancelled() => break,
                else => break,
            }

            log::debug!("Starting to handle tick");
            self.handle_tick().await?;
        }

        Ok(())
    }

    async fn spawn_ws_watcher(&self) -> anyhow::Result<Box<dyn Stream<Item = Log> + Unpin>> {
        let ws_provider = match self.ws_provider {
            Some(ref ws_provider) => ws_provider.clone(),
            None => return Ok(Box::new(stream::empty())),
        };

        let new_multipool_event_filter = self.filter.clone();
        let subscription = ws_provider
            .subscribe_logs(&new_multipool_event_filter)
            .await?;
        Ok(Box::new(subscription.into_stream()))
    }

    async fn handle_tick(&mut self) -> anyhow::Result<()> {
        let last_block_number = self.provider.get_block_number().await?;
        let filter = self
            .filter
            .clone()
            .from_block(self.last_observed_block)
            .to_block(last_block_number - 1);

        log::debug!(
            "Fetching logs from {} to {}",
            self.last_observed_block,
            last_block_number
        );
        let logs = self.provider.get_logs(&filter).await?;

        log::debug!("Updating storage ");
        self.storage
            .insert_logs(
                self.chain_id,
                &logs,
                &self.filter_id,
                last_block_number,
                &mut self.log_processor,
            )
            .await?;

        self.last_observed_block = last_block_number;
        Ok(())
    }
}
