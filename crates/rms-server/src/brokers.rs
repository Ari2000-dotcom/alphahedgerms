//! Talking to the brokers: sign in, read positions, read margin.
//!
//! Sign-in and margin come from the copy-trading engine (`kite-engine`). Positions are
//! read here, one small function per broker. A broker whose positions are not built
//! yet says so plainly instead of showing an empty book.

use kite_engine::kite::{KiteClient, KiteOptions};
use kite_engine::login::{self, BrokerSession, BrokerUrls, Creds};
use kite_engine::Broker;
use rms_db::people::{MarginRow, PositionRow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const BROKERS: [&str; 5] = ["zerodha", "angel", "kotak", "upstox", "nubra"];

/// What is sealed in the database for an account.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredCreds {
    pub user_id: String,
    pub secrets: BTreeMap<String, String>,
    pub fields: BTreeMap<String, String>,
}

impl StoredCreds {
    fn creds(&self) -> Creds<'_> {
        Creds { user_id: &self.user_id, secrets: &self.secrets, fields: &self.fields }
    }
}

/// The details each broker needs before a sign-in is even tried: (name shown, where it lives).
/// Where it lives: 'u' the user id, 's' a secret, 'f' a plain field.
fn required(broker: &str) -> &'static [(&'static str, &'static str, char)] {
    match broker {
        "zerodha" => &[("User ID", "", 'u'), ("API key", "apiKey", 's'), ("API secret", "apiSecret", 's'), ("Web password", "password", 's'), ("TOTP secret", "totpSecret", 's')],
        "angel" => &[("Client code", "", 'u'), ("API key", "apiKey", 's'), ("PIN", "pin", 's'), ("TOTP secret", "totpSecret", 's')],
        "kotak" => &[("UCC", "", 'u'), ("Consumer key", "apiKey", 's'), ("MPIN", "pin", 's'), ("TOTP secret", "totpSecret", 's'), ("Mobile", "mobile", 'f')],
        "upstox" => &[("API key", "apiKey", 's'), ("API secret", "apiSecret", 's'), ("PIN", "pin", 's'), ("TOTP secret", "totpSecret", 's'), ("Mobile", "mobile", 'f'), ("Redirect URL", "redirectUrl", 'f')],
        "nubra" => &[("MPIN", "pin", 's'), ("TOTP secret", "totpSecret", 's'), ("Mobile", "mobile", 'f')],
        _ => &[],
    }
}

/// The names of everything still missing, so the admin is told in one go.
pub fn missing(broker: &str, c: &StoredCreds) -> Vec<&'static str> {
    required(broker)
        .iter()
        .filter(|(_, key, kind)| {
            let v = match kind {
                'u' => c.user_id.as_str(),
                's' => c.secrets.get(*key).map(String::as_str).unwrap_or(""),
                _ => c.fields.get(*key).map(String::as_str).unwrap_or(""),
            };
            v.trim().is_empty()
        })
        .map(|(name, _, _)| *name)
        .collect()
}

/// A broker's own id for the account: the client id, or the mobile where the broker has no id.
pub fn broker_user_id(c: &StoredCreds) -> String {
    if !c.user_id.trim().is_empty() {
        c.user_id.trim().to_string()
    } else {
        c.fields.get("mobile").cloned().unwrap_or_default().trim().to_string()
    }
}

pub async fn sign_in(broker: &str, c: &StoredCreds, urls: &BrokerUrls) -> Result<BrokerSession, String> {
    login::login(broker, &c.creds(), urls, &KiteOptions::default()).await.map_err(|e| e.to_string())
}

/// Margin, read fresh. Brokers that cannot report it yet return an error the screen shows quietly.
pub async fn read_margin(broker: &str, c: &StoredCreds, s: &BrokerSession, urls: &BrokerUrls) -> Result<MarginRow, String> {
    let f = login::funds(broker, &c.creds(), s, urls, &KiteOptions::default()).await.map_err(|e| e.to_string())?;
    Ok(MarginRow { available: f.available, used: f.used, total: f.total, cash: f.cash, collateral: Some(f.collateral), taken_at: String::new() })
}

pub async fn read_positions(broker: &str, c: &StoredCreds, s: &BrokerSession, urls: &BrokerUrls) -> Result<Vec<PositionRow>, String> {
    match broker {
        "zerodha" => zerodha_positions(c, s, urls).await,
        "angel" => angel_positions(c, s, urls).await,
        "kotak" => kotak_positions(s).await,
        other => Err(format!("positions for {other} are not built yet")),
    }
}

async fn zerodha_positions(c: &StoredCreds, s: &BrokerSession, urls: &BrokerUrls) -> Result<Vec<PositionRow>, String> {
    let api = c.secrets.get("apiKey").cloned().unwrap_or_default();
    let opts = KiteOptions { base_url: Some(urls.zerodha_api.clone()), ..Default::default() };
    let client = KiteClient::with_options(api, s.access_token.clone(), opts).map_err(|e| e.to_string())?;
    let p = client.positions().await.map_err(|e| e.to_string())?;
    // The net book: what is open now, carried positions included.
    Ok(p.net
        .into_iter()
        .filter(|x| x.quantity != 0 || x.pnl != 0.0)
        .map(|x| PositionRow {
            trading_symbol: x.tradingsymbol,
            segment: Some(x.exchange),
            token: None,
            product: x.product,
            net_quantity: x.quantity,
            average_price: Some(x.average_price),
            last_price: Some(x.last_price),
            pnl: Some(x.pnl),
        })
        .collect())
}

fn num(v: &Value, key: &str) -> f64 {
    match v.get(key) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().replace(',', "").parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn text(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// `GET getPosition`. Amounts are strings. Net quantity is `netqty`; the average is the buy
/// average for a long and the sell average for a short.
async fn angel_positions(c: &StoredCreds, s: &BrokerSession, urls: &BrokerUrls) -> Result<Vec<PositionRow>, String> {
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)).build().map_err(|e| e.to_string())?;
    let res = http
        .get(format!("{}/rest/secure/angelbroking/order/v1/getPosition", urls.angel.trim_end_matches('/')))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("X-UserType", "USER")
        .header("X-SourceID", "WEB")
        .header("X-ClientLocalIP", "127.0.0.1")
        .header("X-ClientPublicIP", "127.0.0.1")
        .header("X-MACAddress", "00:00:00:00:00:00")
        .header("X-PrivateKey", c.secrets.get("apiKey").cloned().unwrap_or_default())
        .header("Authorization", format!("Bearer {}", s.access_token))
        .send()
        .await
        .map_err(|e| format!("Angel positions: {}", e.without_url()))?;
    let body: Value = res.json().await.map_err(|_| "Angel positions: unexpected reply".to_string())?;
    if body.get("status").and_then(Value::as_bool) != Some(true) {
        return Err(format!("Angel positions: {}", text(&body, "message")));
    }
    let rows = body.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(rows
        .iter()
        .filter_map(|p| {
            let qty = num(p, "netqty") as i64;
            let pnl = num(p, "realised") + num(p, "unrealised");
            if qty == 0 && pnl == 0.0 {
                return None;
            }
            let avg = if qty > 0 { num(p, "totalbuyavgprice") } else if qty < 0 { num(p, "totalsellavgprice") } else { 0.0 };
            Some(PositionRow {
                trading_symbol: text(p, "tradingsymbol"),
                segment: Some(text(p, "exchange")),
                token: Some(text(p, "symboltoken")),
                product: text(p, "producttype"),
                net_quantity: qty,
                average_price: Some(avg),
                last_price: Some(num(p, "ltp")),
                pnl: Some(pnl),
            })
        })
        .collect())
}

/// `GET /quick/user/positions` on the host sign-in returned. Kotak's REST positions reply is not
/// in its public docs, so the fields are read tolerantly (carry-forward plus today's fills).
/// Verify against a live account the first time Kotak's service is answering.
async fn kotak_positions(s: &BrokerSession) -> Result<Vec<PositionRow>, String> {
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)).build().map_err(|e| e.to_string())?;
    let res = http
        .get(format!("{}/quick/user/positions", s.base_url.trim_end_matches('/')))
        .header("accept", "application/json")
        .header("Sid", &s.sid)
        .header("Auth", &s.access_token)
        .header("neo-fin-key", "neotradeapi")
        .send()
        .await
        .map_err(|e| format!("Kotak positions: {}", e.without_url()))?;
    let status = res.status().as_u16();
    if status == 424 || status >= 500 {
        return Err(format!("Kotak's positions service is not answering right now (HTTP {status}). It clears on its own: refresh again later"));
    }
    let body: Value = res.json().await.map_err(|_| format!("Kotak positions: unexpected reply (HTTP {status})"))?;
    let ok = text(&body, "stat").eq_ignore_ascii_case("ok") || body.get("data").is_some();
    if !ok {
        let m = text(&body, "emsg");
        return Err(format!("Kotak positions: {}", if m.is_empty() { format!("HTTP {status}") } else { m }));
    }
    let rows = body.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(rows
        .iter()
        .filter_map(|p| {
            let buy = num(p, "cfBuyQty") + num(p, "flBuyQty");
            let sell = num(p, "cfSellQty") + num(p, "flSellQty");
            let qty = (buy - sell) as i64;
            let buy_amt = num(p, "cfBuyAmt") + num(p, "buyAmt");
            let sell_amt = num(p, "cfSellAmt") + num(p, "sellAmt");
            let ltp = num(p, "lp");
            if qty == 0 && buy_amt == 0.0 && sell_amt == 0.0 {
                return None;
            }
            let (avg, pnl) = if qty > 0 {
                (if buy > 0.0 { buy_amt / buy } else { 0.0 }, sell_amt - buy_amt + qty as f64 * ltp)
            } else if qty < 0 {
                (if sell > 0.0 { sell_amt / sell } else { 0.0 }, sell_amt - buy_amt + qty as f64 * ltp)
            } else {
                (0.0, sell_amt - buy_amt)
            };
            let symbol = if !text(p, "trdSym").is_empty() { text(p, "trdSym") } else { text(p, "sym") };
            Some(PositionRow {
                trading_symbol: symbol,
                segment: Some(text(p, "exSeg")),
                token: Some(text(p, "tok")),
                product: text(p, "prod"),
                net_quantity: qty,
                average_price: Some(avg),
                last_price: Some(ltp),
                pnl: Some(pnl),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds(user: &str, secrets: &[(&str, &str)], fields: &[(&str, &str)]) -> StoredCreds {
        let m = |v: &[(&str, &str)]| v.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        StoredCreds { user_id: user.into(), secrets: m(secrets), fields: m(fields) }
    }

    #[test]
    fn missing_details_are_named_per_broker() {
        let c = creds("AB1234", &[("apiKey", "k")], &[]);
        assert_eq!(missing("zerodha", &c), ["API secret", "Web password", "TOTP secret"]);
        assert_eq!(missing("kotak", &c), ["MPIN", "TOTP secret", "Mobile"]);
        let full = creds("AB1234", &[("apiKey", "k"), ("apiSecret", "s"), ("password", "p"), ("totpSecret", "t")], &[]);
        assert!(missing("zerodha", &full).is_empty());
        assert!(missing("nubra", &creds("", &[("pin", "1"), ("totpSecret", "t")], &[("mobile", "98")])).is_empty());
    }

    #[test]
    fn blanks_count_as_missing() {
        let c = creds("  ", &[("apiKey", "  "), ("pin", ""), ("totpSecret", "t")], &[]);
        let m = missing("angel", &c);
        assert!(m.contains(&"Client code") && m.contains(&"API key") && m.contains(&"PIN") && !m.contains(&"TOTP secret"), "{m:?}");
    }

    #[test]
    fn the_broker_id_falls_back_to_the_mobile() {
        assert_eq!(broker_user_id(&creds("AB1234", &[], &[])), "AB1234");
        assert_eq!(broker_user_id(&creds("", &[], &[("mobile", " 9800000001 ")])), "9800000001");
    }
}
