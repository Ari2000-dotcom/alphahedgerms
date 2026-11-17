//! Plain row types shared by the collectors, the strategy engine and the API.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Broker {
    Zerodha,
    Angel,
    Kotak,
    Upstox,
    Nubra,
}

impl Broker {
    pub fn as_str(self) -> &'static str {
        match self {
            Broker::Zerodha => "zerodha",
            Broker::Angel => "angel",
            Broker::Kotak => "kotak",
            Broker::Upstox => "upstox",
            Broker::Nubra => "nubra",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Buy => "buy",
            Side::Sell => "sell",
        }
    }
    /// +1 for a buy, -1 for a sell: the sign of the quantity it adds to a position.
    pub fn sign(self) -> i64 {
        match self {
            Side::Buy => 1,
            Side::Sell => -1,
        }
    }
}

/// Where a record came from. Kept on every fill so a repaired day is never mistaken for a live one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Live,
    Sweep,
    Import,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Live => "live",
            Source::Sweep => "sweep",
            Source::Import => "import",
        }
    }
}

/// A trade as a broker reports it, before it is stored. `dedupe_key` is built by the broker's
/// collector so the same trade seen twice (live, then in the end-of-day sweep) is stored once.
#[derive(Clone, Debug)]
pub struct NewFill {
    pub account_id: Uuid,
    pub client_id: Uuid,
    pub broker_order_id: String,
    pub exchange: String,
    pub exchange_order_id: Option<String>,
    pub exchange_trade_id: Option<String>,
    pub dedupe_key: String,
    pub segment: Option<String>,
    pub token: Option<String>,
    pub trading_symbol: String,
    pub side: Side,
    pub product: String,
    pub quantity: i64,
    pub price: Decimal,
    pub exchange_ts: Option<DateTime<Utc>>,
    pub broker_ts: Option<DateTime<Utc>>,
    pub source: Source,
    pub raw: serde_json::Value,
}
