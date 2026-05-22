use anyhow::{Ok, Result};
use sqlx::{PgPool, postgres::PgPoolOptions};
use async_trait::async_trait;
use bigdecimal::BigDecimal;

use crate::{application::TransactionRepository, domain::{IndexerState, SolanaTransaction, TransactionEvent}};

pub struct PostgresRepository{
    pool:PgPool
}

impl PostgresRepository{
    pub async fn new(url:&str)->Result<Self>{
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await?;

        Ok(Self { pool })
    }
}

#[async_trait]
impl TransactionRepository for PostgresRepository{

    async fn get_state(&self)->Result<IndexerState>{

        // Returns first row
        let data = sqlx::query!(
            r#"SELECT last_slot,last_block_hash FROM indexer_state WHERE id='main_indexer'"#
        ).fetch_one(&self.pool)
        .await?;

        return Ok(IndexerState { last_slot: data.last_slot as u64, last_block_hash: data.last_block_hash })
    }

    async fn get_last_slot(&self)->Result<u64>{

        // Returns first row
        let data = sqlx::query!(
            r#"SELECT last_slot FROM indexer_state WHERE id='main_indexer'"#
        ).fetch_one(&self.pool)
        .await?;

        return Ok(data.last_slot as u64);
    }

    async fn save_batch(&self,transaction_event:&[TransactionEvent],current_slot:u64)->Result<()>{
        // Start Txn
        let mut txn = self.pool.begin().await?;

        let mut transfers = Vec::new();
        let mut swaps = Vec::new();
        let mut jup_events = Vec::new();
        let mut pump_trades = Vec::new();

        for event in transaction_event{
            match event{
                TransactionEvent::TokenTransfer(transfer)=>{
                    transfers.push(transfer);
                },
                TransactionEvent::RaydiumSwap(swap)=>{
                    swaps.push(swap);
                },
                TransactionEvent::JupiterSwap(jup_event)=>{
                    jup_events.push(jup_event);
                },
                TransactionEvent::PumpFunTrade(trade)=>{
                    pump_trades.push(trade);
                }
            };
        }

        if !transfers.is_empty() {
            // Getting the column values as array
            // Postgres doesn't support u64 , u64 -> BigInt X
            let slots : Vec<i64> = transfers.iter().map(|t| t.slot as i64).collect();
            let amounts: Vec<BigDecimal> = transfers.iter().map(|t| BigDecimal::from(t.amount)).collect();
            let signatures: Vec<String> = transfers.iter().map(|t| t.signature.clone()).collect();
            let senders: Vec<String> =transfers.iter().map(|t| t.from.clone()).collect();
            let receivers: Vec<String> = transfers.iter().map(|t| t.to.clone()).collect();
            let mints: Vec<String> = transfers.iter().map(|t| t.mint.as_deref().unwrap_or("").to_string()).collect();

            // UNNEST takes the array and convert that into rows
            sqlx::query!(
                r#"INSERT INTO token_transfers (signature,sender,receiver,mint,amount,slot)
                SELECT * FROM UNNEST($1::text[],$2::text[],$3::text[],$4::text[],$5::numeric[],$6::bigint[])
                ON CONFLICT DO NOTHING
                "#,
                &signatures,
                &senders,
                &receivers,
                &mints,
                &amounts,
                &slots
            ).execute(&mut *txn)
            .await?;
        }

        if !swaps.is_empty() {
            let slots : Vec<i64> = swaps.iter().map(|s| s.slot as i64).collect();
            let sigs: Vec<String> = swaps.iter().map(|s| s.signature.clone()).collect();
            let pools: Vec<String> = swaps.iter().map(|s| s.amm_pool.clone()).collect();
            let users: Vec<String> = swaps.iter().map(|s| s.signer.clone()).collect();
            let amts_in: Vec<BigDecimal> = swaps.iter().map(|s| BigDecimal::from(s.amount_in)).collect();
            let min_outs: Vec<BigDecimal> = swaps.iter().map(|s| BigDecimal::from(s.min_amount_out)).collect();

            let amts_received: Vec<BigDecimal> = swaps.iter().map(|s| BigDecimal::from(s.amount_received)).collect();
            let mints_src: Vec<String> = swaps.iter().map(|s| s.mint_source.clone()).collect();
            let mints_dst: Vec<String> = swaps.iter().map(|s| s.mint_destination.clone()).collect();

            sqlx::query!(
                r#"INSERT INTO raydium_swaps (signature, amm_pool, sender, amount_in, min_amount_out,amount_received,mint_source,mint_destination, slot)
                SELECT * FROM UNNEST($1::text[], $2::text[], $3::text[], $4::numeric[], $5::numeric[],$6::numeric[],$7::text[],$8::text[], $9::bigint[])
                ON CONFLICT (signature, amm_pool) DO NOTHING"#,
                &sigs, &pools, &users, &amts_in, &min_outs,&amts_received,&mints_src,&mints_dst ,&slots
            )
            .execute(&mut *txn)
            .await?;

            tracing::info!("✅ Saved batch of {} swaps to DB", swaps.len())
        }

        if !jup_events.is_empty() {
            sqlx::query!(
                r#"
                INSERT INTO jupiter_swaps (
                    signature, slot, block_time, signer, amm_pool,
                    mint_in, mint_out, amount_in, amount_out,
                    slippage_bps, platform_fee_bps, route_plan
                )
                SELECT * FROM UNNEST(
                    $1::text[], $2::bigint[], $3::timestamp[], $4::text[], $5::text[],
                    $6::text[], $7::text[], $8::numeric[], $9::numeric[],
                    $10::int[], $11::int[], $12::jsonb[]
                )
                ON CONFLICT (signature) DO NOTHING
                "#,
                &jup_events.iter().map(|e| e.signature.clone()).collect::<Vec<_>>(),
                &jup_events.iter().map(|e| e.slot as i64).collect::<Vec<_>>(),
                &jup_events.iter().map(|e| chrono::DateTime::from_timestamp(e.block_time, 0).unwrap().naive_utc()).collect::<Vec<_>>(),
                &jup_events.iter().map(|e| e.signer.clone()).collect::<Vec<_>>(),
                &jup_events.iter().map(|e| e.amm_pool.clone()).collect::<Vec<_>>(),
                &jup_events.iter().map(|e| e.mint_in.clone()).collect::<Vec<_>>(),
                &jup_events.iter().map(|e| e.mint_out.clone()).collect::<Vec<_>>(),
                &jup_events.iter().map(|e| BigDecimal::from(e.amount_in)).collect::<Vec<_>>(),
                &jup_events.iter().map(|e| BigDecimal::from(e.amount_out)).collect::<Vec<_>>(),
                &jup_events.iter().map(|e| e.slippage_bps as i32).collect::<Vec<_>>(),
                &jup_events.iter().map(|e| e.platform_fee_bps as i32).collect::<Vec<_>>(),
                &jup_events.iter().map(|e| serde_json::to_value(&e.route_plan).unwrap()).collect::<Vec<_>>()
            )
            .execute(&mut *txn)
            .await?;
        }

        if !pump_trades.is_empty(){
            sqlx::query!(
                r#"INSERT INTO pump_fun_trades
                (signature, slot, block_time, mint, is_buy, user_address, token_amount, sol_amount)
                SELECT * FROM UNNEST($1::text[], $2::bigint[], $3::timestamp[], $4::text[], $5::boolean[], $6::text[], $7::numeric[], $8::numeric[])
                ON CONFLICT (signature, mint) DO NOTHING"#,
                &pump_trades.iter().map(|t| t.signature.clone()).collect::<Vec<_>>(),
                &pump_trades.iter().map(|t| t.slot as i64).collect::<Vec<_>>(),
               &pump_trades.iter().map(|t| chrono::DateTime::from_timestamp(t.block_time, 0).unwrap().naive_utc()).collect::<Vec<_>>(),
                &pump_trades.iter().map(|t| t.mint.clone()).collect::<Vec<_>>(),
                &pump_trades.iter().map(|t| t.is_buy).collect::<Vec<_>>(),
                &pump_trades.iter().map(|t| t.user.clone()).collect::<Vec<_>>(),
                &pump_trades.iter().map(|t| BigDecimal::from(t.token_amount)).collect::<Vec<_>>(),
                &pump_trades.iter().map(|t| BigDecimal::from(t.sol_amount)).collect::<Vec<_>>()
            )
            .execute(&self.pool)
            .await?;
        }


        sqlx::query!(
            r#"UPDATE indexer_state SET last_slot = $1, last_block_hash = 'TODO' WHERE id = 'main_indexer'"#,
            current_slot as i64
        ).execute(&mut *txn)
        .await?;

        // Commit all
        txn.commit().await?;

        tracing::info!("✅ Saved batch of {} transfers to DB", transfers.len());

        Ok(())


    }

    async fn save_dlq(&self,txn:&SolanaTransaction,parser_name:&str,error:&str)->Result<()>{
        let tx_json = serde_json::to_value(txn)?;

        sqlx::query!(
            r#"
            INSERT INTO transaction_dlq (signature, slot, parser_name, error_msg, tx_data)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (signature) DO UPDATE
            SET error_msg = $4, retry_count = transaction_dlq.retry_count + 1
            "#,
            txn.signature,
            txn.slot as i64,
            parser_name,
            error,
            tx_json
        ).execute(&self.pool)
        .await?;

        Ok(())
    }
}
