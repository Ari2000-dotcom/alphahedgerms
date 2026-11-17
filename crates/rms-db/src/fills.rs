//! Storing trades. The one rule: the same trade seen twice is stored once.

use crate::model::NewFill;
use crate::Result;
use sqlx::PgPool;

/// Stores a trade. Returns the new row's id, or `None` when this trade was already stored
/// (seen live, then again in the end-of-day sweep, or in an imported statement).
pub async fn insert_fill(pool: &PgPool, f: &NewFill) -> Result<Option<i64>> {
    let id: Option<(i64,)> = sqlx::query_as(
        "INSERT INTO fills (account_id, client_id, broker_order_id, exchange, exchange_order_id, exchange_trade_id, dedupe_key,
                            segment, token, trading_symbol, side, product, quantity, price, exchange_ts, broker_ts, source, raw)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
         ON CONFLICT (account_id, dedupe_key) DO NOTHING
         RETURNING id",
    )
    .bind(f.account_id)
    .bind(f.client_id)
    .bind(&f.broker_order_id)
    .bind(&f.exchange)
    .bind(&f.exchange_order_id)
    .bind(&f.exchange_trade_id)
    .bind(&f.dedupe_key)
    .bind(&f.segment)
    .bind(&f.token)
    .bind(&f.trading_symbol)
    .bind(f.side.as_str())
    .bind(&f.product)
    .bind(f.quantity)
    .bind(f.price)
    .bind(f.exchange_ts)
    .bind(f.broker_ts)
    .bind(f.source.as_str())
    .bind(&f.raw)
    .fetch_optional(pool)
    .await?;
    Ok(id.map(|r| r.0))
}
