use std::time::Duration;

use crate::indexer1::{Indexer, Processor};

use alloy::{
    network::EthereumWallet,
    node_bindings::Anvil,
    primitives::{Address, U256},
    providers::ProviderBuilder,
    rpc::types::Filter,
    signers::local::PrivateKeySigner,
    sol,
    sol_types::SolEvent,
};
use anyhow::Result;
use sqlx::{Database, SqlitePool};
use tokio::sync::mpsc;

// Codegen from embedded Solidity code and precompiled bytecode.
// solc v0.8.26; solc Counter.sol --via-ir --optimize --bin
sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    MockERC20,
    "src/tests/artifacts/MockERC20.json"
);

pub struct TestProcessor {
    terminate_tx: mpsc::UnboundedSender<()>,
}

impl<'a, DB: Database> Processor<sqlx::Transaction<'a, DB>> for TestProcessor {
    async fn process(
        &mut self,
        logs: &[alloy::rpc::types::Log],
        _transaction: &mut sqlx::Transaction<'a, DB>,
        _prev_saved_block: u64,
        _new_saved_block: u64,
        _chain_id: u64,
    ) -> anyhow::Result<()> {
        println!("{logs:?}");
        self.terminate_tx.send(())?;
        Ok(())
    }
}

#[sqlx::test]
async fn happy_path(pool: SqlitePool) -> Result<()> {
    let anvil = Anvil::new().block_time_f64(0.1).try_spawn()?;

    let signer: PrivateKeySigner = anvil.keys()[0].clone().into();
    let wallet = EthereumWallet::from(signer);

    // Create a provider.
    let ws = alloy::providers::WsConnect::new(anvil.ws_endpoint());
    let provider = ProviderBuilder::new().wallet(wallet).on_ws(ws).await?;

    let contract = MockERC20::deploy(
        provider,
        "name".to_string(),
        "symbol".to_string(),
        U256::from(10000),
    )
    .await?;

    contract
        .transfer(Address::from([3; 20]), U256::from(1))
        .send()
        .await?
        .watch()
        .await?;

    let contract_address = *contract.address();
    let ws_url = anvil.ws_endpoint_url().clone();
    let http_url = anvil.endpoint_url().clone();

    let (terminate_tx, mut terminate_rx) = mpsc::unbounded_channel();

    let handle = tokio::spawn(async move {
        Indexer::builder()
            .http_rpc_url(http_url)
            .ws_rpc_url(ws_url)
            .fetch_interval(Duration::from_secs(10))
            .filter(Filter::new().address(contract_address).events([
                MockERC20::Transfer::SIGNATURE,
                MockERC20::Approval::SIGNATURE,
            ]))
            .sqlite_storage(pool)
            .set_processor(TestProcessor { terminate_tx })
            .build()
            .await
            .unwrap()
            .run()
            .await
            .unwrap();
    });

    terminate_rx.recv().await;
    handle.abort();
    Ok(())
}
