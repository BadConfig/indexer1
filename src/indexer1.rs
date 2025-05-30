//! Indexer implementation
use std::time::Duration;

use alloy::{
    eips::BlockNumberOrTag,
    primitives::keccak256,
    providers::Provider,
    rpc::types::{Filter, Log},
    sol_types::SolValue,
};
use anyhow::Context;
use futures::{stream, Future, Stream, StreamExt};
use sha2::{Digest, Sha256};
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;

use crate::{builder::IndexerBuilder, storage::LogStorage};

pub trait Processor<T> {
    /// Function is invoked every time new logs are found. They are guaranteed to be ordered and
    /// not duplicated if applied to database through given transaction.
    fn process(
        &mut self,
        logs: &[Log],
        transaction: &mut T,
        prev_saved_block: u64,
        new_saved_block: u64,
        chain_id: u64,
    ) -> impl Future<Output = anyhow::Result<()>>;
}

#[derive(Debug, Clone, Copy)]
/// Finality level for the indexer. It defines which block to use as a reference point for
/// fetching logs.
pub enum FinalityLevel {
    Finalized,
    Safe,
    Latest,
    Pending,
}

impl From<FinalityLevel> for BlockNumberOrTag {
    fn from(level: FinalityLevel) -> Self {
        match level {
            FinalityLevel::Finalized => BlockNumberOrTag::Finalized,
            FinalityLevel::Safe => BlockNumberOrTag::Safe,
            FinalityLevel::Latest => BlockNumberOrTag::Latest,
            FinalityLevel::Pending => BlockNumberOrTag::Pending,
        }
    }
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

pub struct Indexer<S: LogStorage, P: Processor<S::Transaction>> {
    pub(crate) chain_id: u64,
    pub(crate) filter_id: String,
    pub(crate) filter: Filter,
    pub(crate) last_observed_block: u64,
    pub(crate) storage: S,
    pub(crate) log_processor: P,
    pub(crate) http_provider: Box<dyn Provider>,
    pub(crate) ws_provider: Option<Box<dyn Provider>>,
    pub(crate) fetch_interval: Duration,
    pub(crate) overtake_interval: Duration,
    pub(crate) block_range_limit: Option<u64>,
    pub(crate) finality_level: BlockNumberOrTag,
}

impl<S: LogStorage, P: Processor<S::Transaction>> Indexer<S, P> {
    pub fn builder() -> IndexerBuilder<S, P> {
        IndexerBuilder::default()
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        let mut poll_interval = IntervalStream::new(interval(self.fetch_interval));
        let mut ws_watcher = self.spawn_ws_watcher().await?;

        log::info!("Succesfully initialised watcher and poller");
        loop {
            tokio::select! {
                Some(_) = ws_watcher.next() => {}
                Some(_) = poll_interval.next() => {}
                else => break,
            }

            log::debug!("Starting to handle tick");
            loop {
                let did_reach_latest_block = self.handle_tick().await?;
                if did_reach_latest_block {
                    break;
                } else {
                    tokio::time::sleep(self.overtake_interval).await;
                }
            }
        }

        Ok(())
    }

    async fn spawn_ws_watcher(&self) -> anyhow::Result<Box<dyn Stream<Item = Log> + Unpin + Send>> {
        let ws_provider = match self.ws_provider {
            Some(ref ws_provider) => ws_provider,
            None => return Ok(Box::new(stream::empty())),
        };

        let new_multipool_event_filter = self.filter.clone();
        let subscription = ws_provider
            .subscribe_logs(&new_multipool_event_filter)
            .await?;
        Ok(Box::new(subscription.into_stream()))
    }

    async fn handle_tick(&mut self) -> anyhow::Result<bool> {
        let from_block = self.last_observed_block + 1;
        let latest_block = self
            .http_provider
            .get_block_by_number(self.finality_level)
            .await?
            .context("No finalized block")?
            .header
            .number;
        if self.last_observed_block == latest_block {
            return Ok(true); // in case of no new blocks
        }
        let to_block = self
            .block_range_limit
            .map(|block_range_limit| latest_block.min(from_block + block_range_limit))
            .unwrap_or(latest_block);

        let filter = self
            .filter
            .clone()
            .from_block(from_block)
            .to_block(alloy::eips::BlockNumberOrTag::Number(to_block));

        log::debug!("Fetching logs from {} to {}", from_block, to_block);
        let logs = self.http_provider.get_logs(&filter).await?;

        log::debug!("Updating storage ");
        self.storage
            .insert_logs(
                self.chain_id,
                &logs,
                &self.filter_id,
                from_block,
                to_block,
                &mut self.log_processor,
            )
            .await?;

        self.last_observed_block = to_block;
        Ok(to_block == latest_block)
    }
}
