//!# Indexer1
//!
//!Indexer1 is a library that provides a mechanism to index EVM-compatible blockchains.  
//!It uses PostgreSQL to ensure consistent and fault-tolerant indexing, however re-implementing storage trait is available.
//!Indexer1 is lightweight, self-hosted and simple to use.
//!If you need a professional and extendable way of indexing EVM, you should definately prefer Indexer1
//!
//!# Example
//!To make it work you only need to implement a Processor trait that gives you transaction and
//!allow you to inject your custom logic of parsing and applying logs. It also gives you a
//!transaction to work with database and does all the deduplication work for you. You are
//!guaranteed to receive all events ordered.
//!
//!```ignore
//!pub struct TestProcessor;
//!
//!impl Processor for TestProcessor {
//!    async fn process<Postgres>(
//!        &mut self,
//!        _logs: &[alloy::rpc::types::Log],
//!        _transaction: &mut sqlx::Transaction<'static, Postgres>,
//!        _chain_id: u64,
//!    ) -> anyhow::Result<()> {
//!        // put here any code to collect logs
//!        Ok(())
//!    }
//!}
//!```
//!
//!After you described processing, build indexer by specifying all connections and filter for
//!events. If the filter will be changed, indexer will re-index all the data from the blockchain.
//!
//!```ignore
//!Indexer::builder()
//!    .http_rpc_url(http_url)
//!    .ws_rpc_url(ws_url)
//!    .fetch_interval(Duration::from_secs(10))
//!    // Add event filter
//!    .filter(Filter::new().address(contract_address).events([
//!        MockERC20::Transfer::SIGNATURE,
//!        MockERC20::Approval::SIGNATURE,
//!    ]))
//!    // Set up a function to process events
//!    .set_processor(TestProcessor)
//!    .sqlite_storage(pool)
//!    .build()
//!    .await
//!    .unwrap()
//!    .run()
//!    .await?;
//!```
pub use alloy;
pub use anyhow;
pub use builder::IndexerBuilder;
pub use indexer1::{Indexer, Processor};
pub use sqlx;
pub use storage::LogStorage;
pub use tokio;
pub use tokio_util;

mod builder;
mod indexer1;
mod storage;
#[cfg(test)]
mod tests;
