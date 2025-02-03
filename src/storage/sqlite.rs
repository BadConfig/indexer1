use alloy::rpc::types::{Filter, Log};
use sqlx::{Pool, Row, Sqlite};

use crate::indexer1::{filter_id, Processor};

use super::LogStorage;

impl LogStorage for Pool<Sqlite> {
    type Transaction = sqlx::Transaction<'static, Sqlite>;

    async fn insert_logs<P: Processor<Self::Transaction>>(
        &self,
        chain_id: u64,
        logs: &[Log],
        filter_id: &str,
        prev_saved_block: u64,
        new_saved_block: u64,
        log_processor: &mut P,
    ) -> anyhow::Result<()> {
        let mut transaction = self.begin().await?;
        log_processor
            .process(
                logs,
                &mut transaction,
                prev_saved_block,
                new_saved_block,
                chain_id,
            )
            .await?;
        sqlx::query(include_str!("sql/update_filter.sql"))
            .bind::<i64>(new_saved_block.try_into()?)
            .bind(filter_id)
            .execute(&mut *transaction)
            .await?;

        transaction.commit().await.map_err(Into::into)
    }

    async fn get_or_create_filter(
        &self,
        filter: &Filter,
        chain_id: u64,
    ) -> anyhow::Result<(u64, String)> {
        sqlx::query(include_str!("sql/create_filter.sql"))
            .execute(self)
            .await?;

        let filter_id = filter_id(filter, chain_id);
        let last_observed_block = sqlx::query(include_str!("sql/get_filter.sql"))
            .bind(&filter_id)
            .fetch_optional(self)
            .await?
            .map(|row| {
                row.get::<i64, _>(0)
                    .try_into()
                    .map(|v| (v, filter_id.clone()))
            })
            .transpose()?;
        match last_observed_block {
            Some((block, filter_id)) => Ok((block, filter_id)),
            None => sqlx::query(include_str!("sql/insert_filter.sql"))
                .bind(&filter_id)
                .bind::<i64>(filter.get_from_block().unwrap_or(1).try_into()?)
                .bind(serde_json::to_value(filter)?)
                .execute(self)
                .await
                .map_err(Into::into)
                .map(|_| (filter.get_from_block().unwrap_or(1), filter_id)),
        }
    }
}
