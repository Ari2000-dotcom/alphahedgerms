# Alphahedge RMS: design

A risk-management system for Indian F&O brokers. An admin connects broker
accounts for their clients and sees every account's positions, margins, ledger,
orders and trades in one place. The system keeps its own permanent record of
every order and trade, so a strategy grouped by the admin stays whole after the
broker has forgotten the day.

Status: design agreed, storage moved from Postgres to DuckDB plus Parquet (2026-11-17), database layer and admin screen built, broker positions read for Zerodha, Angel One and Kotak.
Advisor review: Opus 5.5, 2026-11-17. Its corrections are marked (advisor).

## 1. Goals and non-goals

Phase 1 (admin only):
- Add broker accounts (Zerodha, Kotak Neo, Angel One, Upstox, Nubra) for clients.
- See all positions, MTM, margins, ledger and orders across accounts, live.
- Keep every order and fill forever, in our own database.
- Group orders into strategies (tags) and keep them correct across days.
- Show where our record and the broker disagree.

Later phases: client portal (a client sees only their own data), onboarding,
risk limits with alerts and then actions, intraday order-book review.

Non-goals for phase 1: placing, changing or cancelling orders. The RMS only
reads. This is deliberate: it keeps the first release safe.

## 2. Roles

- Admin: adds accounts, tags strategies, sees everything. Several admins are
  possible, with a read-only "risk officer" role.
- Client (later): sees only their own accounts and strategies.

Every row that belongs to a client carries `client_id` from day one, so the
portal needs no schema change. When the portal ships, Postgres row-level
security on `client_id` becomes a second safety net behind the API checks.

## 3. Architecture

```
                     +-------------------------------+
   Browser (admin)<--+  rms-server (Rust, axum)       |
   REST + WebSocket  |   API, auth, audit            |
                     |   strategy engine             |
                     |   reconciler                  |
                     |   collectors (one per account)|
                     +--+-----------+-----------+----+
                        |           |           |
              broker APIs   broker order    DuckDB file + Parquet archive
              (REST)        WebSockets      (one folder, written only by the server)
                        |
              hosted market feed (MarketHub /ws, quote:SEG:token) for live prices
```

Crates (a Cargo workspace):
- `rms-db`: DuckDB schema, queries, Parquet export and the plain data types. No network code.
- `rms-brokers`: sign-in (reused from `kite-engine`), margin, positions,
  orders, trades, order streams. One module per broker behind one trait.
- `rms-server`: the process: API, collectors, reconciler, strategy engine.
- `web/`: the admin screen (Preact + Vite), same grey and sky look as the desk.

Reused from the copy-trading engine: the encrypted account vault, TOTP
sign-in for all five brokers, margin reads, the Kite order socket.

## 4. Data capture

Brokers keep only today: the order book and trade book reset overnight and
intraday positions vanish. So capture is the first thing built and the most
important job of the system. (advisor: a day not recorded is gone.)

Per account, a collector runs:
1. Sign in at start and after the broker's daily token reset (Kite 06:00 IST,
   Upstox about 03:30). If sign-in fails by 09:00 an alert is raised.
2. Open the broker's order WebSocket. Every order event writes an
   `order_events` row, and a new fill triggers a trade-book read for that
   account (not a timer). This keeps load on the brokers low.
3. Poll positions and margins on a slow timer, and once about a second after
   any order event, because no broker streams margin. This is how margin stays
   close to live.
4. At close (15:45 IST) run the end-of-day sweep: read the whole order book,
   trade book and positions once more and store anything missing. This is
   mandatory. If the server was down at 15:30, the sweep still finds the day.
5. Every fill records its `source`: live, sweep or import.

Rate limits are per broker, enforced by one token bucket per broker so many
accounts cannot exceed them together.

Gap repair: a tradebook / contract-note CSV import from each broker's
back office, deduplicated against the same natural key. (advisor)

### Broker matrix

| Broker | Orders | Trades | Positions | Margin | Order stream |
|---|---|---|---|---|---|
| Zerodha | GET /orders | GET /trades | GET /portfolio/positions | GET /user/margins | Kite WebSocket, `type: order` frames (already built) |
| Angel One | GET getOrderBook | GET getTradeBook | GET getPosition | GET getRMS | SmartAPI order-status socket (verify the URL and headers when building) |
| Kotak Neo | GET /quick/user/orders | GET /quick/user/trades | position stream and `/quick/user/positions` (verify) | POST /quick/user/limits | `wss://<baseUrl>/realtime`, raw `{type:cn,Authorization:..,Sid:..,src:WEB}` then `type: order` and `type: position` frames |
| Upstox | GET order book | GET trades for day | GET positions | GET funds-and-margin | portfolio stream |
| Nubra | to be documented | to be documented | to be documented | none yet | to be documented |

Kotak quirks: amounts are strings; trades have no trade id, so the fill's dedupe
key is built from order number, exchange order id, exchange time, quantity and
price; dedupe order events on `nOrdNo + updRecvTm`; a token expiry ends the
socket, so reconnect with a fresh sign-in.

## 5. The database

DuckDB, with Parquet for finished history. No database server to install or run.

- One DuckDB file (`data/rms.duckdb`) holds everything that changes and everything
  still active: clients, accounts, strategies, tags, live fills and orders.
- Finished history is exported to ZSTD-compressed Parquet files
  (`data/fills/date=YYYY-MM-DD/fills.parquet`). Files are written to a temporary
  name and renamed, never edited, so they are a tamper-evident archive and back up
  by copying. Measured: 3,000 synthetic fills became 28 KB (about 9.5 bytes each).
- Only one process may write to the file: `rms-server`. The admin screen and the
  later client portal talk to the server, never to the file. A client sees only
  their own rows because every query goes through one function that filters by
  `client_id`. (This replaces Postgres row-level security.)
- DuckDB has no triggers and its foreign keys cannot update a referenced row, so:
  the data layer offers only "insert" for the record tables; `history_fingerprint()`
  hashes fills, order events and assignment events so a nightly check can prove
  nothing changed behind its back; `orphans()` replaces foreign keys.
- Money is `DECIMAL(18,4)`, never floating point. Values go in as text and are cast
  in SQL, so nothing depends on how a driver maps decimals or times.
- Retention: a setting, `heavy_data_days_after_expiry` (0 = keep). Raw broker replies
  and snapshots are the heavy part. A short permanent summary per finished strategy
  (`strategy_summaries`, about 1 KB) is what remains. The job itself is not built;
  the owner has not yet chosen the number of days.

Tables:
- `admins`, `clients`, `broker_accounts` (credentials sealed with the vault).
- `instruments` keyed by `(segment, token)`, the exchange token that is the same
  across brokers and is also the hosted feed's key (`quote:NSE_FO:43721`);
  `instrument_aliases` maps each broker's own symbol to it.
- `orders` (current state) and `order_events` (full history, including rejected
  and cancelled orders, which never touch legs).
- `fills`: one row per trade. Natural key `(account, exchange, exchange_order_id,
  exchange_trade_id)`, with a broker-specific `dedupe_key` where a broker has no
  trade id. Exchange timestamp preferred; an ingest sequence breaks ties; the raw
  JSON of every fill is kept. (advisor)
- `position_events`: changes that are not fills: `conversion` (MIS to NRML),
  `expiry_settle`, `exercise`, `delivery`, `import_open`, `manual_adjust`.
  Without it every expiry looks like a reconciliation error. (advisor)
- `strategies`, `assignment_events` (append-only audit), `current_assignments`
  (the live answer, written in the same transaction), `assignment_rules`.
- `lots` and `lot_closures`: derived, rebuildable caches of strategy P&L.
- `position_snapshots`, `margin_snapshots`, `ledger_entries`, `recon_flags`,
  `collector_state`.

## 6. Strategies

A strategy belongs to one client and has a tag. The admin attaches **orders**
(not single fills) to it, so every later partial fill of an order follows
automatically. A single fill can still be overridden. (advisor)

Lots are keyed by `(strategy, account, instrument, product)` and hold a signed
quantity; side is derived. A buy closes a short. Closing fills are matched FIFO
inside the strategy only. If two strategies hold the same contract in one
account, nothing is matched automatically: the fill goes to the Unassigned inbox.
Auto-attach happens only when exactly one strategy has an open lot of the
opposite sign. Rules ("all NIFTY weekly orders in account X go to strategy S")
keep the inbox short. (advisor)

Re-tagging is a new `assignment_event`, never an edit. Strategy P&L is
recomputed from fills plus current assignments, only for the strategies a change
touches. This is "half" event-sourcing on purpose: an audit trail and a cache
that can be rebuilt, without a replay engine. (advisor)

### The straddle, day by day

Day 1: the client sells a CE and a PE. The admin tags both orders to S1. The PE
is bought back the same day.
- S1: PE lot opened and closed (realised P&L kept), CE lot open.

Day 2: the broker shows only the CE. The RMS still shows S1 whole: the closed PE
leg with its realised P&L and the open CE. The client sells another call. It
appears in the Unassigned inbox; the admin attaches the order to S1, and the
change is recorded with admin, time and reason.

## 7. Reconciliation

Every few seconds, per `(account, instrument, product)`, in units (not lots, since
lot sizes get revised): broker net quantity must equal the sum of open lots
(assigned and unassigned) plus position events. A difference opens a `recon_flag`.
We alert on quantity only, never on P&L, because brokers price carried positions
differently. P&L is shown both ways, labelled. (advisor)

If the system starts mid-stream, the broker's net positions are imported as
`import_open` events and marked as imported.

## 8. Margin, funds and ledger

- Margin (available including collateral, used, total, cash, collateral): read
  from the broker, stored as `margin_snapshots`, refreshed on order events.
- Funds snapshot: what the margin read gives. It is a balance, not a ledger.
- Ledger: Zerodha, Angel and Kotak do not offer a ledger through their APIs.
  The ledger comes from importing the broker's back-office ledger CSV
  (`ledger_entries`). The screen labels the two clearly. (advisor)

## 9. Live prices

Mark-to-market uses the hosted MarketHub feed (`/ws`, an API key, topics
`quote:<SEGMENT>:<token>`). The server holds one connection and subscribes only
to contracts that are open somewhere; the admin screen receives the merged
result. Broker symbols are mapped to exchange tokens through the instrument
master.

## 10. Security

- Admin sign-in with password and TOTP; sessions; audit log of every action.
- Broker credentials sealed in the database with the vault (AES-256-GCM); the
  key lives outside the database.
- Reading a saved secret back needs an explicit action and is logged.
- The server listens on one port behind HTTPS; one pinned egress IP per broker
  app, documented, because brokers enforce static IPs. (advisor)
- Brokers' terms on automated sign-in must be checked per broker. (advisor)

## 11. Operations

Backups: copy the `data` folder (checkpoint first; the Parquet files never change, so copies are incremental); test a restore. Alerts for:
sign-in failure, collector silent for more than a minute in market hours, a
reconciliation flag, the sweep not finishing. The server must be on every market
day, ideally on a small always-on machine.

## 12. Build order

1. Capture daemon: orders and fills for every account, raw payloads, natural key,
   idempotent upserts, end-of-day sweep. Start the clock first. (advisor)
2. Admin grid: accounts x live positions, MTM, margins, a drift flag.
3. Strategy tagging, the Unassigned inbox, rules, the strategy view, expiry
   handling. This is the straddle demo.
4. Then roles, ledger import and the client portal.
5. Later: onboarding, limits, the order-book area.

## 13. Open questions for the owner

1. Where does it run: this PC or a server? It must be on every market day.
2. How many accounts and clients now, and in a year?
3. Web screen or desktop app? (This design assumes a web screen.)
4. Where are the contract/instrument CSV files from the copy trading backend?
5. Can ledger and tradebook CSVs be downloaded from each broker's back office? A
   sample of each is needed.
6. One admin or several; is a read-only risk officer needed?
7. Does a client ever tag strategies, or only the admin? Can a strategy close and
   reopen?
8. Which limits matter first, and alert only or act?
9. How many years of history must be kept?
