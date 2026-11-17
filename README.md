# Alphahedge RMS

A risk-management system for Indian F&O brokers. An admin connects broker
accounts for clients and sees every account's positions, margins, ledger,
orders and trades in one place. The system keeps its own permanent record of
every order and trade, so strategies the admin groups stay whole after the
broker has forgotten the day.

Read `docs/DESIGN.md` first. It is the plan; this file is the state.

## State

| Part | State |
|---|---|
| Design | written (docs/DESIGN.md), reviewed by an advisor |
| Database | DuckDB + Parquet: schema, fills without duplicates, tamper fingerprint, Parquet export, clients, accounts, position and margin snapshots. 20 tests |
| Server | admin login, clients, broker accounts (sealed), sign-in, positions and margin read for Zerodha, Angel One, Kotak (Kotak positions unverified) |
| Admin screen | built: sign-in, overview, Clients grid (Add client = one inline row with the broker's sign-in columns and hints, Enter saves and signs in; the same client name adds another broker; saved rows editable), client page, positions |
| Upstox and Nubra positions | not built |
| Order sockets, trade capture, end-of-day sweep | not built (next) |
| Strategies, tagging, reconciliation | not built |
| Live prices from the MarketHub feed | not built (positions use the broker's own last price) |
| Retention job (delete after expiry) | not built; the owner has not chosen the days |

## Layout

```
crates/rms-db/      DuckDB schema, queries, Parquet export
crates/rms-server/  the server: API, brokers, admin login
web/                the admin screen (Preact + Vite)
docs/DESIGN.md      the design
```

## Run

```bash
cd web && npm install && npm run build      # once: build the admin screen
cargo run -p rms-server                     # serves the screen and API on 127.0.0.1:8090
```

The first run creates `.env` with a random admin password and the key that seals broker
passwords, and prints the password once. Keep `.env` private. Open
http://127.0.0.1:8090 and sign in.

Development: `cargo run -p rms-server` plus `cd web && npm run dev` (the page on :5290,
its /api calls proxied to the server).

The `target` folder is large (DuckDB's engine is compiled from source, about 8 GB per
build profile). It can be deleted at any time.
