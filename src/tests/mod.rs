use std::time::Duration;

use crate::indexer1::{Indexer, Processor};

use alloy::{
    node_bindings::Anvil,
    primitives::{Address, U256},
    providers::ProviderBuilder,
    rpc::types::Filter,
    sol,
    sol_types::SolEvent,
};
use anyhow::Result;
use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;

// Codegen from embedded Solidity code and precompiled bytecode.
// solc v0.8.26; solc Counter.sol --via-ir --optimize --bin
sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    MockERC20,
    "src/tests/artifacts/MockERC20.json"
);

pub struct TestProcessor {
    cancel_token: CancellationToken,
}

impl Processor for TestProcessor {
    async fn process<DB: sqlx::Database>(
        &mut self,
        logs: &[alloy::rpc::types::Log],
        _transaction: &mut sqlx::Transaction<'static, DB>,
        _chain_id: u64,
    ) -> anyhow::Result<()> {
        println!("{logs:?}");
        self.cancel_token.cancel();
        Ok(())
    }
}

#[sqlx::test]
async fn happy_path(pool: SqlitePool) -> Result<()> {
    let anvil = Anvil::new().block_time(1).try_spawn()?;

    // Create a provider.
    let ws = alloy::providers::WsConnect::new(anvil.ws_endpoint());
    let provider = ProviderBuilder::new().on_ws(ws).await?;

    let contract = MockERC20::deploy(
        provider,
        "name".to_string(),
        "symbol".to_string(),
        U256::from(10000),
    )
    .await?;

    contract
        .transfer(Address::ZERO, U256::from(1))
        .send()
        .await?
        .watch()
        .await?;

    let contract_address = *contract.address();
    let ws_url = anvil.ws_endpoint_url().clone();
    let http_url = anvil.endpoint_url().clone();

    let token = CancellationToken::new();

    Indexer::builder()
        .http_rpc_url(http_url)
        .ws_rpc_url(ws_url)
        .fetch_interval(Duration::from_secs(10))
        .filter(Filter::new().address(contract_address).events([
            MockERC20::Transfer::SIGNATURE,
            MockERC20::Approval::SIGNATURE,
        ]))
        .set_processor(TestProcessor {
            cancel_token: token.clone(),
        })
        .sqlite_storage(pool)
        .build()
        .await
        .unwrap()
        .run(token)
        .await?;
    Ok(())
}
