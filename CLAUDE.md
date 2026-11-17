# Alphahedge RMS

Admin-first risk-management system. The admin adds broker accounts for clients and sees their positions, margins, ledger and orders. It keeps its own permanent record of every order and fill. Strategies (tags) group orders per client and must survive the brokers' daily resets. The user portal, onboarding, limits and order book come later. Full design: `docs/DESIGN.md`. State of the build: `README.md`.

## Layout

- `crates/rms-db` (DuckDB, bundled): schema in `sql/0001_init.sql`, `lib.rs` (Db, migrations, history fingerprint, orphan check, Parquet export), `fills.rs`, `people.rs` (clients, accounts, snapshots), tests in `tests/`.
- `crates/rms-server` (axum 0.7): `api.rs` routes and admin auth, `brokers.rs` sign-in and position/margin reads, `setup.rs` first-run secrets, `main.rs`. Depends by path on `D:\Alphahedgecopytrading\kite-engine` (Vault, login, KiteClient, Broker trait).
- `web/` (Preact 11, @preact/signals, Vite 8, TypeScript): `app.tsx` (shell, overview, client page, positions), `clients.tsx` (the Clients grid), `brokers.ts` (what each broker needs, hints, grid columns), `api.ts`, `ui.tsx`, `styles.css`, `dev-mock.ts` (sample data).

## Run

```
cd web && npm run build
cargo run -p rms-server        # serves the site and API on 127.0.0.1:8090
```

First run writes `.env` with `RMS_ADMIN_PASSWORD` and `RMS_VAULT_KEY`. Env: `RMS_DATA_DIR` (default `data`), `RMS_BIND`, `RMS_WEB_DIR` (default `web/dist`). `target` can reach about 16 GB and is safe to delete.

Look at the screens without a server: `cd web && npx vite --port 5290`, then open `http://127.0.0.1:5290/?mock`. Stop it by process id, never with a broad `taskkill` on node.

## Decisions to keep

- Storage is DuckDB plus compressed Parquet, not Postgres. No triggers or foreign keys: `orphans()` checks instead. `at` is a reserved word, so columns are `occurred_at`. Bind values as text and CAST in SQL. Fills dedupe on `UNIQUE(account_id, dedupe_key)`. `history_fingerprint()` detects tampering. `0001_init.sql` was edited in place only because no real data existed: treat it as immutable from now on, add new migrations.
- Data is deleted after expiry. `heavy_data_days_after_expiry` is 0 (off) until the user answers how many days, and whether a small per-strategy summary stays forever. Still unanswered.
- Admin login is a password from `.env`, an HttpOnly SameSite=Strict cookie `rms_session`, 12 hour sessions in memory, a 1 second wait on a wrong password. Broker sessions live in memory only. Credentials are sealed with the Vault in `broker_accounts.credentials`. Secrets are never sent back to the page, only whether each is saved.
- Add client works like the copy-trading desk: one inline row with client name, broker and that broker's sign-in columns. Enter saves and signs in. The same client name attaches another broker. Saved rows edit in place and only typed values replace saved ones. Routes: `POST /api/accounts` (client and account in one step), `PUT /api/accounts/:id`.
- Position reads: Zerodha, Angel One, Kotak (Kotak REST positions unverified). Upstox and Nubra positions are not built. Kotak margin fails with HTTP 424 on Kotak's side.
- Live prices are to come from the hosted MarketHub feed. Trade capture is to use the brokers' order WebSockets (Zerodha Kite, Kotak `wss://<baseUrl>/realtime`, Angel order-status socket). Neither is built.

## Web rules

- Tables use `rtable`, `rrow`, `gtable`. Do not use the desk's `grid-table` or `row` names: they clash.
- Preact: split password and text inputs (shared `common` props) because of the `type` union. State updates must be functional (`setF(p => ...)`) because autofill sets several fields at once.
- Design: neutral grey ladder with no tint, one light sky accent for meaning (active page, primary button, focus), sage green and terracotta for profit and loss, Inter at weights 400 and 600, no bold scattered through text. Figure cards, a labelled side bar, initials avatars, status pills, a bottom tab bar under 760 px. Check any UI change against the redesign skill.
- Keep the mock data in `dev-mock.ts` realistic (no fake round numbers).

## Tests

`cargo test -p rms-db` (20 tests; the 3000 fill Parquet test takes about a minute), `cargo test -p rms-server` (7 tests), `cd web && npm run build` (type-check and build).

## Open items

Trade capture and end-of-day sweep, fills and orders ingestion, strategy tagging and lots and reconciliation, Upstox and Nubra positions, MarketHub live prices, ledger CSV import, the retention job, batched inserts for bulk loads, Kotak positions check, risk limits, client portal, onboarding. Questions for the user: where it runs (PC or server), how many accounts and clients, admin roles, tagging rules, limits, retention.

## Git

Commits are local. Nothing is pushed unless the user asks.
