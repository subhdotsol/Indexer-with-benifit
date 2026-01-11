# Phase 2: Parsing Logic & Multi-Protocol Support

In Phase 2, we moved beyond just "reading" transactions to "understanding" them.

---

## 1. The Strategy: The "Parser" Trait

Instead of writing one giant `if/else` block in the pipeline, we used the **Strategy Pattern**. Each protocol (SPL Token, Raydium, Jupiter) gets its own isolated parser.

### The Trait Definitions
We defined this in `src/application/ports/parser.rs`:
```rust
#[async_trait]
pub trait TransactionParser: Send + Sync {
    fn parse(&self, txn: &SolanaTransaction) -> Option<Vec<TransactionEvent>>;
    fn name(&self) -> &str;
}
```
**Why?**
1. **Isolation**: Adding a "Pump.fun" parser won't break the "Raydium" parser.
2. **Extensibility**: We can add 100 parsers, and the `IngestionPipeline` stays exactly the same.

---

## 2. Protocol Breakdown

### SPL Token Parser
- **Target**: `TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA`
- **Logic**: It looks for instruction discriminators `3` (Transfer) and `12` (TransferChecked).
- **Data**: It extracts `from`, `to`, `amount`, and `mint` (if available).

### Raydium AMM Parser
- **Target**: `675k1P952h926S9fXN1v8B2YyW6QiX48zSt5q35XV55`
- **Logic**: Decodes the `SwapInstructionBaseIn` (Opcode 9).
- **Complexity**: Real implementation requires parsing "Inner Instructions" to find exactly how much was received after fees/slippage.

### Jupiter Parser
- **Target**: `JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4`
- **Logic**: Handles various "Route" discriminators. Since Jupiter aggregates others, it's often more complex than standard AMMs.

---

## 3. How the Pipeline Uses Them
In `src/application/use_cases/ingest.rs`, we now do this:
```rust
for parser in &self.parsers {
    if let Some(events) = parser.parse(&txn) {
        for event in events {
            tracing::info!("Parser [{}] found event: {:?}", parser.name(), event);
        }
    }
}
```
This is a "Broadcast" model. Every transaction is offered to every parser. If a parser recognizes it, it returns events.

---

## 4. Why the New Dependencies?
- **`solana-sdk` / `solana-transaction-status`**: Core types for Solana accounts and transaction metadata.
- **`borsh`**: Binary Serializer for Solana (used by most programs for instruction data).
- **`bs58`**: Solana uses Base58 for everything (signatures, addresses).
- **`prost`**: Handles Protocol Buffers (used for gRPC data).
