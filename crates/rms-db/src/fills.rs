//! Storing trades. The one rule: the same trade seen twice is stored once.

use crate::model::NewFill;
use crate::{Db, Result};
use duckdb::{params, OptionalExt};

impl Db {
    /// Stores a trade. Returns the new row's id, or `None` when this trade was already stored
    /// (seen live, then again in the end-of-day sweep, or in an imported statement).
    ///
    /// Values go in as text and are cast in SQL, so nothing depends on how a driver maps
    /// decimals or times.
    pub fn insert_fill(&self, f: &NewFill) -> Result<Option<i64>> {
        self.with(|c| {
            let id = c
                .prepare_cached(
                    "INSERT INTO fills (account_id, client_id, broker_order_id, exchange, exchange_order_id, exchange_trade_id, dedupe_key,
                                        segment, token, trading_symbol, side, product, quantity, price, exchange_ts, broker_ts, source, raw)
                     VALUES (CAST(? AS UUID), CAST(? AS UUID), ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CAST(? AS DECIMAL(18, 4)),
                             CAST(? AS TIMESTAMPTZ), CAST(? AS TIMESTAMPTZ), ?, ?)
                     ON CONFLICT (account_id, dedupe_key) DO NOTHING
                     RETURNING id",
                )?
                .query_row(
                    params![
                        f.account_id.to_string(),
                        f.client_id.to_string(),
                        f.broker_order_id,
                        f.exchange,
                        f.exchange_order_id,
                        f.exchange_trade_id,
                        f.dedupe_key,
                        f.segment,
                        f.token,
                        f.trading_symbol,
                        f.side.as_str(),
                        f.product,
                        f.quantity,
                        f.price.to_string(),
                        f.exchange_ts.map(|t| t.to_rfc3339()),
                        f.broker_ts.map(|t| t.to_rfc3339()),
                        f.source.as_str(),
                        f.raw.to_string(),
                    ],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?;
            Ok(id)
        })
    }

    /// How many fills are stored.
    pub fn fill_count(&self) -> Result<i64> {
        self.with(|c| Ok(c.query_row("SELECT count(*) FROM fills", [], |r| r.get(0))?))
    }
}
