use anyhow::{Ok, Result};
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};

use crate::{
    application::TransactionRepository,
    domain::{IndexerState, SolanaTransaction, TransactionEvent},
};

pub struct PostgresRepository {
    pool: PgPool,
}

impl PostgresRepository {
    pub async fn new(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl TransactionRepository for PostgresRepository {
    async fn get_state(&self) -> Result<IndexerState> {
        let row = sqlx::query(
            "SELECT last_slot, last_block_hash FROM indexer_state WHERE id = 'main_indexer'"
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(IndexerState {
            last_slot: row.try_get::<i64, _>("last_slot")? as u64,
            last_block_hash: row.try_get("last_block_hash")?,
        })
    }

    async fn get_last_slot(&self) -> Result<u64> {
        let row = sqlx::query(
            "SELECT last_slot FROM indexer_state WHERE id = 'main_indexer'"
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.try_get::<i64, _>("last_slot")? as u64)
    }

    async fn save_batch(&self, events: &[TransactionEvent], current_slot: u64) -> Result<()> {
        let mut txn = self.pool.begin().await?;

        let mut transfers = Vec::new();
        let mut raydium_swaps = Vec::new();
        let mut jupiter_swaps = Vec::new();
        let mut pump_trades = Vec::new();

        for ev in events {
            match ev {
                TransactionEvent::TokenTransfer(t) => transfers.push(t),
                TransactionEvent::RaydiumSwap(s) => raydium_swaps.push(s),
                TransactionEvent::JupiterSwap(s) => jupiter_swaps.push(s),
                TransactionEvent::PumpFunTrade(t) => pump_trades.push(t),
            }
        }

        if !transfers.is_empty() {
            let slots: Vec<i64>          = transfers.iter().map(|t| t.slot as i64).collect();
            let amounts: Vec<BigDecimal> = transfers.iter().map(|t| BigDecimal::from(t.amount)).collect();
            let sigs: Vec<String>        = transfers.iter().map(|t| t.signature.clone()).collect();
            let senders: Vec<String>     = transfers.iter().map(|t| t.from.clone()).collect();
            let receivers: Vec<String>   = transfers.iter().map(|t| t.to.clone()).collect();
            let mints: Vec<String>       = transfers.iter().map(|t| t.mint.as_deref().unwrap_or("").to_string()).collect();

            sqlx::query(
                r#"INSERT INTO token_transfers (signature, sender, receiver, mint, amount, slot)
                   SELECT * FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[], $5::numeric[], $6::bigint[])
                   ON CONFLICT DO NOTHING"#,
            )
            .bind(&sigs)
            .bind(&senders)
            .bind(&receivers)
            .bind(&mints)
            .bind(&amounts)
            .bind(&slots)
            .execute(&mut *txn)
            .await?;
        }

        if !raydium_swaps.is_empty() {
            let sigs:      Vec<String>     = raydium_swaps.iter().map(|s| s.signature.clone()).collect();
            let pools:     Vec<String>     = raydium_swaps.iter().map(|s| s.amm_pool.clone()).collect();
            let users:     Vec<String>     = raydium_swaps.iter().map(|s| s.signer.clone()).collect();
            let amts_in:   Vec<BigDecimal> = raydium_swaps.iter().map(|s| BigDecimal::from(s.amount_in)).collect();
            let min_outs:  Vec<BigDecimal> = raydium_swaps.iter().map(|s| BigDecimal::from(s.min_amount_out)).collect();
            let received:  Vec<BigDecimal> = raydium_swaps.iter().map(|s| BigDecimal::from(s.amount_received)).collect();
            let mints_src: Vec<String>     = raydium_swaps.iter().map(|s| s.mint_source.clone()).collect();
            let mints_dst: Vec<String>     = raydium_swaps.iter().map(|s| s.mint_destination.clone()).collect();
            let slots:     Vec<i64>        = raydium_swaps.iter().map(|s| s.slot as i64).collect();

            sqlx::query(
                r#"INSERT INTO raydium_swaps
                   (signature, amm_pool, sender, amount_in, min_amount_out, amount_received, mint_source, mint_destination, slot)
                   SELECT * FROM UNNEST($1::text[], $2::text[], $3::text[], $4::numeric[], $5::numeric[], $6::numeric[], $7::text[], $8::text[], $9::bigint[])
                   ON CONFLICT (signature, amm_pool) DO NOTHING"#,
            )
            .bind(&sigs)
            .bind(&pools)
            .bind(&users)
            .bind(&amts_in)
            .bind(&min_outs)
            .bind(&received)
            .bind(&mints_src)
            .bind(&mints_dst)
            .bind(&slots)
            .execute(&mut *txn)
            .await?;

            tracing::info!("Saved {} Raydium swaps", raydium_swaps.len());
        }

        if !jupiter_swaps.is_empty() {
            let sigs:      Vec<String>     = jupiter_swaps.iter().map(|e| e.signature.clone()).collect();
            let slots_:    Vec<i64>        = jupiter_swaps.iter().map(|e| e.slot as i64).collect();
            let times:     Vec<chrono::NaiveDateTime> = jupiter_swaps.iter()
                .map(|e| chrono::DateTime::from_timestamp(e.block_time, 0).unwrap().naive_utc())
                .collect();
            let signers:   Vec<String>     = jupiter_swaps.iter().map(|e| e.signer.clone()).collect();
            let pools:     Vec<String>     = jupiter_swaps.iter().map(|e| e.amm_pool.clone()).collect();
            let mints_in:  Vec<String>     = jupiter_swaps.iter().map(|e| e.mint_in.clone()).collect();
            let mints_out: Vec<String>     = jupiter_swaps.iter().map(|e| e.mint_out.clone()).collect();
            let amts_in:   Vec<BigDecimal> = jupiter_swaps.iter().map(|e| BigDecimal::from(e.amount_in)).collect();
            let amts_out:  Vec<BigDecimal> = jupiter_swaps.iter().map(|e| BigDecimal::from(e.amount_out)).collect();
            let slippages: Vec<i32>        = jupiter_swaps.iter().map(|e| e.slippage_bps as i32).collect();
            let fees:      Vec<i32>        = jupiter_swaps.iter().map(|e| e.platform_fee_bps as i32).collect();
            let routes:    Vec<serde_json::Value> = jupiter_swaps.iter()
                .map(|e| serde_json::to_value(&e.route_plan).unwrap())
                .collect();

            sqlx::query(
                r#"INSERT INTO jupiter_swaps
                   (signature, slot, block_time, signer, amm_pool, mint_in, mint_out,
                    amount_in, amount_out, slippage_bps, platform_fee_bps, route_plan)
                   SELECT * FROM UNNEST(
                       $1::text[], $2::bigint[], $3::timestamp[], $4::text[], $5::text[],
                       $6::text[], $7::text[], $8::numeric[], $9::numeric[],
                       $10::int[], $11::int[], $12::jsonb[]
                   )
                   ON CONFLICT (signature) DO NOTHING"#,
            )
            .bind(&sigs)
            .bind(&slots_)
            .bind(&times)
            .bind(&signers)
            .bind(&pools)
            .bind(&mints_in)
            .bind(&mints_out)
            .bind(&amts_in)
            .bind(&amts_out)
            .bind(&slippages)
            .bind(&fees)
            .bind(&routes)
            .execute(&mut *txn)
            .await?;
        }

        if !pump_trades.is_empty() {
            let sigs:    Vec<String>     = pump_trades.iter().map(|t| t.signature.clone()).collect();
            let slots_:  Vec<i64>        = pump_trades.iter().map(|t| t.slot as i64).collect();
            let times:   Vec<chrono::NaiveDateTime> = pump_trades.iter()
                .map(|t| chrono::DateTime::from_timestamp(t.block_time, 0).unwrap().naive_utc())
                .collect();
            let mints:   Vec<String>     = pump_trades.iter().map(|t| t.mint.clone()).collect();
            let is_buys: Vec<bool>       = pump_trades.iter().map(|t| t.is_buy).collect();
            let users:   Vec<String>     = pump_trades.iter().map(|t| t.user.clone()).collect();
            let tokens:  Vec<BigDecimal> = pump_trades.iter().map(|t| BigDecimal::from(t.token_amount)).collect();
            let sols:    Vec<BigDecimal> = pump_trades.iter().map(|t| BigDecimal::from(t.sol_amount)).collect();

            sqlx::query(
                r#"INSERT INTO pump_fun_trades
                   (signature, slot, block_time, mint, is_buy, user_address, token_amount, sol_amount)
                   SELECT * FROM UNNEST($1::text[], $2::bigint[], $3::timestamp[], $4::text[], $5::boolean[], $6::text[], $7::numeric[], $8::numeric[])
                   ON CONFLICT (signature, mint) DO NOTHING"#,
            )
            .bind(&sigs)
            .bind(&slots_)
            .bind(&times)
            .bind(&mints)
            .bind(&is_buys)
            .bind(&users)
            .bind(&tokens)
            .bind(&sols)
            .execute(&mut *txn)
            .await?;
        }

        sqlx::query(
            "UPDATE indexer_state SET last_slot = $1, last_block_hash = 'TODO' WHERE id = 'main_indexer'"
        )
        .bind(current_slot as i64)
        .execute(&mut *txn)
        .await?;

        txn.commit().await?;

        tracing::info!("Batch committed: {} transfers, {} raydium, {} jupiter, {} pump",
            transfers.len(), raydium_swaps.len(), jupiter_swaps.len(), pump_trades.len());

        Ok(())
    }

    async fn save_dlq(&self, txn: &SolanaTransaction, parser_name: &str, error: &str) -> Result<()> {
        let tx_json = serde_json::to_value(txn)?;

        sqlx::query(
            r#"INSERT INTO transaction_dlq (signature, slot, parser_name, error_msg, tx_data)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (signature) DO UPDATE
               SET error_msg = $4, retry_count = transaction_dlq.retry_count + 1"#,
        )
        .bind(&txn.signature)
        .bind(txn.slot as i64)
        .bind(parser_name)
        .bind(error)
        .bind(tx_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
