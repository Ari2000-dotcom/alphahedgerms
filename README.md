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
| Database schema | written (`crates/rms-db/migrations/0001_init.sql`) and compiles |
| Database tests | written, **not yet run against a real Postgres** (they skip without one) |
| Server | connects, migrates, answers `/health` |
| Broker collectors, order sockets | not built |
| Strategy engine, reconciler | not built |
| Admin screen | not built |

## Layout

```
crates/rms-db/      schema, migrations, row types, storing fills
crates/rms-server/  the server process
scripts/setup-db.ps1  creates the RMS's own Postgres cluster (port 5433)
docs/DESIGN.md      the design
```

## Run

```bash
powershell -ExecutionPolicy Bypass -File scripts\setup-db.ps1   # once: own Postgres + .env
cargo run -p rms-server -- migrate                               # apply the schema
cargo run -p rms-server                                          # serve on RMS_BIND
RMS_TEST_DATABASE_URL=postgres://... cargo test -p rms-db        # database tests
```

`.env` holds the database password and is never committed.
