//! PostgreSQL Repository Adapter
//!
//! This module implements the `EventRepository` trait for PostgreSQL database.
//! It provides persistence for all parsed transaction events:
//! - Token transfers (SPL Token)
//! - Raydium AMM swaps
//! - Jupiter aggregator swaps  
//! - PumpFun bonding curve swaps
//!
//! Each event type is stored in its own table with ON CONFLICT DO NOTHING
//! to handle duplicate signatures gracefully.

use crate::{
    application::{AppError, AppResult, EventRepository},
    domain::{
        JupiterSwapEvent, PumpFunSwapEvent, RaydiumSwapEvent, TokenTransfer, TransactionEvent,
    },
};
use async_trait::async_trait;
use sqlx::{postgres::PgPoolOptions, PgPool};

pub struct PostgresRepository {
    pool: PgPool,
}

impl PostgresRepository {
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    async fn save_token_transfer(&self, transfer: &TokenTransfer) -> AppResult<()> {
        sqlx::query(
            r#"INSERT INTO token_transfers (signature, slot, from_address, to_address, amount, mint)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT (signature) DO NOTHING"#,
        )
        .bind(&transfer.signature)
        .bind(transfer.slot as i64)
        .bind(&transfer.from)
        .bind(&transfer.to)
        .bind(transfer.amount as i64)
        .bind(&transfer.mint)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        tracing::debug!(signature = %transfer.signature, "Saved token transfer");
        Ok(())
    }

    async fn save_raydium_swap(&self, swap: &RaydiumSwapEvent) -> AppResult<()> {
        sqlx::query(
            r#"INSERT INTO raydium_swaps (signature, slot, amm_pool, signer, amount_in, min_amount_out, amount_received, mint_source, mint_destination)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               ON CONFLICT (signature) DO NOTHING"#,
        )
        .bind(&swap.signature)
        .bind(swap.slot as i64)
        .bind(&swap.amm_pool)
        .bind(&swap.signer)
        .bind(swap.amount_in as i64)
        .bind(swap.min_amount_out as i64)
        .bind(swap.amount_received as i64)
        .bind(&swap.mint_source)
        .bind(&swap.mint_destination)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        tracing::debug!(signature = %swap.signature, "Saved Raydium swap");
        Ok(())
    }

    async fn save_jupiter_swap(&self, swap: &JupiterSwapEvent) -> AppResult<()> {
        sqlx::query(
            r#"INSERT INTO jupiter_swaps (signature, slot, signer, amm_pool, mint_in, mint_out, amount_in, amount_out, slippage_bps)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               ON CONFLICT (signature) DO NOTHING"#,
        )
        .bind(&swap.signature)
        .bind(swap.slot as i64)
        .bind(&swap.signer)
        .bind(&swap.amm_pool)
        .bind(&swap.mint_in)
        .bind(&swap.mint_out)
        .bind(swap.amount_in as i64)
        .bind(swap.amount_out as i64)
        .bind(swap.slippage_bps as i16)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        tracing::debug!(signature = %swap.signature, "Saved Jupiter swap");
        Ok(())
    }

    async fn save_pumpfun_swap(&self, swap: &PumpFunSwapEvent) -> AppResult<()> {
        sqlx::query(
            r#"INSERT INTO pumpfun_swaps (signature, slot, signer, mint, is_buy, sol_amount, token_amount, bonding_curve)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (signature) DO NOTHING"#,
        )
        .bind(&swap.signature)
        .bind(swap.slot as i64)
        .bind(&swap.signer)
        .bind(&swap.mint)
        .bind(swap.is_buy)
        .bind(swap.sol_amount as i64)
        .bind(swap.token_amount as i64)
        .bind(&swap.bonding_curve)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        tracing::debug!(signature = %swap.signature, "Saved PumpFun swap");
        Ok(())
    }
}

#[async_trait]
impl EventRepository for PostgresRepository {
    async fn save_event(&self, event: &TransactionEvent) -> AppResult<()> {
        match event {
            TransactionEvent::TokenTransfer(t) => self.save_token_transfer(t).await,
            TransactionEvent::RaydiumSwap(s) => self.save_raydium_swap(s).await,
            TransactionEvent::JupiterSwap(s) => self.save_jupiter_swap(s).await,
            TransactionEvent::PumpFunSwap(s) => self.save_pumpfun_swap(s).await,
        }
    }

    async fn save_events(&self, events: &[TransactionEvent]) -> AppResult<()> {
        for event in events {
            self.save_event(event).await?;
        }
        Ok(())
    }

    /// Batch insert events using a single transaction for better performance.
    /// This is optimized for the background queue persistence pattern.
    async fn save_events_batch(&self, events: Vec<TransactionEvent>) -> AppResult<usize> {
        if events.is_empty() {
            return Ok(0);
        }

        let count = events.len();

        // Use a transaction for atomicity and better performance
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        for event in &events {
            match event {
                TransactionEvent::TokenTransfer(transfer) => {
                    sqlx::query(
                        r#"INSERT INTO token_transfers (signature, slot, from_address, to_address, amount, mint)
                           VALUES ($1, $2, $3, $4, $5, $6)
                           ON CONFLICT (signature) DO NOTHING"#,
                    )
                    .bind(&transfer.signature)
                    .bind(transfer.slot as i64)
                    .bind(&transfer.from)
                    .bind(&transfer.to)
                    .bind(transfer.amount as i64)
                    .bind(&transfer.mint)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::DatabaseError(e.to_string()))?;
                }
                TransactionEvent::RaydiumSwap(swap) => {
                    sqlx::query(
                        r#"INSERT INTO raydium_swaps (signature, slot, amm_pool, signer, amount_in, min_amount_out, amount_received, mint_source, mint_destination)
                           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                           ON CONFLICT (signature) DO NOTHING"#,
                    )
                    .bind(&swap.signature)
                    .bind(swap.slot as i64)
                    .bind(&swap.amm_pool)
                    .bind(&swap.signer)
                    .bind(swap.amount_in as i64)
                    .bind(swap.min_amount_out as i64)
                    .bind(swap.amount_received as i64)
                    .bind(&swap.mint_source)
                    .bind(&swap.mint_destination)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::DatabaseError(e.to_string()))?;
                }
                TransactionEvent::JupiterSwap(swap) => {
                    sqlx::query(
                        r#"INSERT INTO jupiter_swaps (signature, slot, signer, amm_pool, mint_in, mint_out, amount_in, amount_out, slippage_bps)
                           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                           ON CONFLICT (signature) DO NOTHING"#,
                    )
                    .bind(&swap.signature)
                    .bind(swap.slot as i64)
                    .bind(&swap.signer)
                    .bind(&swap.amm_pool)
                    .bind(&swap.mint_in)
                    .bind(&swap.mint_out)
                    .bind(swap.amount_in as i64)
                    .bind(swap.amount_out as i64)
                    .bind(swap.slippage_bps as i16)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::DatabaseError(e.to_string()))?;
                }
                TransactionEvent::PumpFunSwap(swap) => {
                    sqlx::query(
                        r#"INSERT INTO pumpfun_swaps (signature, slot, signer, mint, is_buy, sol_amount, token_amount, bonding_curve)
                           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                           ON CONFLICT (signature) DO NOTHING"#,
                    )
                    .bind(&swap.signature)
                    .bind(swap.slot as i64)
                    .bind(&swap.signer)
                    .bind(&swap.mint)
                    .bind(swap.is_buy)
                    .bind(swap.sol_amount as i64)
                    .bind(swap.token_amount as i64)
                    .bind(&swap.bonding_curve)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::DatabaseError(e.to_string()))?;
                }
            }
        }

        tx.commit()
            .await
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        tracing::info!(count = count, "Batch persisted events to database");
        Ok(count)
    }
}
