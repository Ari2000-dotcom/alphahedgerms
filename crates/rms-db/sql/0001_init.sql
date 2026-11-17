-- Alphahedge RMS: initial schema, for DuckDB.
--
-- Rules the whole schema follows:
--   * Money and prices are DECIMAL, never floating point.
--   * Quantities are in units (not lots), as BIGINT. Signed: positive long, negative short.
--   * The record of what happened (fills, order_events, position_events,
--     assignment_events, audit_log) is append-only. DuckDB has no triggers, so the
--     data layer offers only "insert" for these tables, and history_fingerprint()
--     lets a nightly check prove nothing was changed behind its back.
--   * No FOREIGN KEY constraints: DuckDB cannot update a referenced row cleanly.
--     Integrity is checked in code (Db::orphans).
--   * Every row that belongs to a client carries client_id, so a client portal can
--     filter by it in one place without a schema change.
--   * Tables marked "derived" are caches that can be rebuilt from the record.
--   * Raw broker replies are kept as JSON text in a `raw` column. They are the heavy
--     part and the first thing the retention job removes after expiry.
--   * Times are stored in UTC (TIMESTAMPTZ).

-- ---------------------------------------------------------------- people

CREATE TABLE admins (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email           VARCHAR NOT NULL UNIQUE,
    name            VARCHAR NOT NULL,
    password_hash   VARCHAR NOT NULL,
    totp_sealed     BLOB,
    role            VARCHAR NOT NULL DEFAULT 'admin' CHECK (role IN ('admin', 'risk_readonly')),
    disabled        BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- A client owns one or more broker accounts: the "user" of the later portal.
CREATE TABLE clients (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    client_code     VARCHAR NOT NULL UNIQUE,
    name            VARCHAR NOT NULL,
    email           VARCHAR,
    phone           VARCHAR,
    status          VARCHAR NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'inactive')),
    notes           VARCHAR,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE broker_accounts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    client_id       UUID NOT NULL,
    broker          VARCHAR NOT NULL CHECK (broker IN ('zerodha', 'angel', 'kotak', 'upstox', 'nubra')),
    broker_user_id  VARCHAR NOT NULL,                -- client id / UCC at the broker
    label           VARCHAR NOT NULL,
    credentials     BLOB NOT NULL,                   -- sealed with the vault, never plain
    enabled         BOOLEAN NOT NULL DEFAULT TRUE,
    created_by      UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (broker, broker_user_id)
);

-- ------------------------------------------------------------ instruments

-- Keyed by the exchange token: the same at every broker and the key the hosted
-- market feed uses (quote:NSE_FO:43721).
CREATE TABLE instruments (
    segment         VARCHAR NOT NULL,                -- NSE_EQ, NSE_FO, BSE_FO, MCX_FO ...
    token           VARCHAR NOT NULL,
    trading_symbol  VARCHAR NOT NULL,
    name            VARCHAR,
    underlying      VARCHAR,
    instrument_type VARCHAR NOT NULL CHECK (instrument_type IN ('EQ', 'FUT', 'CE', 'PE', 'INDEX', 'OTHER')),
    expiry          DATE,
    strike          DECIMAL(18, 4),
    lot_size        INTEGER,
    tick_size       DECIMAL(18, 4),
    updated_on      DATE NOT NULL DEFAULT current_date,
    PRIMARY KEY (segment, token)
);
CREATE INDEX instruments_symbol ON instruments (trading_symbol);

-- Each broker's own symbol for an instrument.
CREATE TABLE instrument_aliases (
    broker          VARCHAR NOT NULL,
    broker_symbol   VARCHAR NOT NULL,
    segment         VARCHAR NOT NULL,
    token           VARCHAR NOT NULL,
    PRIMARY KEY (broker, broker_symbol)
);

-- ---------------------------------------------------------------- orders

CREATE SEQUENCE orders_id_seq;
CREATE SEQUENCE order_events_id_seq;
CREATE SEQUENCE fills_id_seq;
CREATE SEQUENCE position_events_id_seq;
CREATE SEQUENCE assignment_events_id_seq;
CREATE SEQUENCE assignment_rules_id_seq;
CREATE SEQUENCE lots_id_seq;
CREATE SEQUENCE lot_closures_id_seq;
CREATE SEQUENCE position_runs_id_seq;
CREATE SEQUENCE position_snapshots_id_seq;
CREATE SEQUENCE margin_snapshots_id_seq;
CREATE SEQUENCE ledger_entries_id_seq;
CREATE SEQUENCE recon_flags_id_seq;
CREATE SEQUENCE audit_log_id_seq;

-- Current state of each order (updated as events arrive).
CREATE TABLE orders (
    id                  BIGINT PRIMARY KEY DEFAULT nextval('orders_id_seq'),
    account_id          UUID NOT NULL,
    client_id           UUID NOT NULL,
    broker_order_id     VARCHAR NOT NULL,
    exchange_order_id   VARCHAR,
    parent_order_id     BIGINT,                      -- child orders of a bracket or cover
    segment             VARCHAR,
    token               VARCHAR,
    trading_symbol      VARCHAR NOT NULL,
    side                VARCHAR NOT NULL CHECK (side IN ('buy', 'sell')),
    product             VARCHAR NOT NULL,            -- MIS, NRML, CNC ...
    order_type          VARCHAR NOT NULL,
    quantity            BIGINT NOT NULL,
    price               DECIMAL(18, 4),
    trigger_price       DECIMAL(18, 4),
    status              VARCHAR NOT NULL,
    filled_quantity     BIGINT NOT NULL DEFAULT 0,
    average_price       DECIMAL(18, 4),
    tag                 VARCHAR,
    placed_at           TIMESTAMPTZ,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    raw                 VARCHAR,
    UNIQUE (account_id, broker_order_id)
);
CREATE INDEX orders_client_day ON orders (client_id, placed_at);

-- Every state an order went through, including rejections and cancellations.
-- These never touch strategy lots. Append-only.
CREATE TABLE order_events (
    id                  BIGINT PRIMARY KEY DEFAULT nextval('order_events_id_seq'),
    account_id          UUID NOT NULL,
    broker_order_id     VARCHAR NOT NULL,
    status              VARCHAR NOT NULL,
    filled_quantity     BIGINT,
    average_price       DECIMAL(18, 4),
    reason              VARCHAR,
    event_key           VARCHAR NOT NULL,            -- broker-specific; makes the insert idempotent
    observed_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    source              VARCHAR NOT NULL CHECK (source IN ('live', 'poll', 'sweep', 'import')),
    raw                 VARCHAR,
    UNIQUE (account_id, event_key)
);
CREATE INDEX order_events_order ON order_events (account_id, broker_order_id);

-- ----------------------------------------------------------------- fills

-- One row per trade: the source of truth for quantity and price. Append-only.
CREATE TABLE fills (
    id                  BIGINT PRIMARY KEY DEFAULT nextval('fills_id_seq'),   -- also the ingest sequence for tie-breaks
    account_id          UUID NOT NULL,
    client_id           UUID NOT NULL,
    broker_order_id     VARCHAR NOT NULL,
    exchange            VARCHAR NOT NULL,
    exchange_order_id   VARCHAR,
    exchange_trade_id   VARCHAR,
    -- Natural key. Where a broker gives no trade id (Kotak) the collector builds it
    -- from order number, exchange order id, exchange time, quantity and price.
    dedupe_key          VARCHAR NOT NULL,
    segment             VARCHAR,
    token               VARCHAR,
    trading_symbol      VARCHAR NOT NULL,
    side                VARCHAR NOT NULL CHECK (side IN ('buy', 'sell')),
    product             VARCHAR NOT NULL,
    quantity            BIGINT NOT NULL CHECK (quantity > 0),
    price               DECIMAL(18, 4) NOT NULL,
    exchange_ts         TIMESTAMPTZ,                 -- preferred; some brokers lack seconds
    broker_ts           TIMESTAMPTZ,
    source              VARCHAR NOT NULL CHECK (source IN ('live', 'sweep', 'import')),
    raw                 VARCHAR,
    ingested_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (account_id, dedupe_key)
);
CREATE INDEX fills_order ON fills (account_id, broker_order_id);
CREATE INDEX fills_instrument_time ON fills (account_id, segment, token, exchange_ts);
CREATE INDEX fills_client_time ON fills (client_id, exchange_ts);

-- Position changes that are not trades. Without these every expiry, conversion and
-- imported opening position would look like a reconciliation error. Append-only.
CREATE TABLE position_events (
    id              BIGINT PRIMARY KEY DEFAULT nextval('position_events_id_seq'),
    account_id      UUID NOT NULL,
    client_id       UUID NOT NULL,
    kind            VARCHAR NOT NULL CHECK (kind IN ('conversion', 'expiry_settle', 'exercise', 'delivery', 'import_open', 'manual_adjust')),
    segment         VARCHAR NOT NULL,
    token           VARCHAR NOT NULL,
    product         VARCHAR NOT NULL,
    quantity_delta  BIGINT NOT NULL,                 -- signed units
    price           DECIMAL(18, 4),                  -- settlement or opening price
    effective_at    TIMESTAMPTZ NOT NULL,
    note            VARCHAR,
    created_by      UUID,                            -- null when made by the system
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX position_events_instrument ON position_events (account_id, segment, token);

-- ------------------------------------------------------------ strategies

CREATE TABLE strategies (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    client_id       UUID NOT NULL,
    tag             VARCHAR NOT NULL,
    name            VARCHAR NOT NULL,
    status          VARCHAR NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
    notes           VARCHAR,
    created_by      UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at       TIMESTAMPTZ,
    UNIQUE (client_id, tag)
);

-- The short, permanent record of a finished strategy (about 1 KB): what the
-- retention job leaves behind when it removes the heavy data after expiry.
CREATE TABLE strategy_summaries (
    strategy_id     UUID PRIMARY KEY,
    client_id       UUID NOT NULL,
    tag             VARCHAR NOT NULL,
    name            VARCHAR NOT NULL,
    opened_at       TIMESTAMPTZ,
    closed_at       TIMESTAMPTZ,
    expired_on      DATE,
    realised_pnl    DECIMAL(18, 4) NOT NULL,
    legs            VARCHAR NOT NULL,                -- JSON text: instrument, side, quantity, entry, exit, P&L per leg
    summarised_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Every tagging, re-tagging and un-tagging, with who and why. Append-only.
CREATE TABLE assignment_events (
    id              BIGINT PRIMARY KEY DEFAULT nextval('assignment_events_id_seq'),
    client_id       UUID NOT NULL,
    subject_kind    VARCHAR NOT NULL CHECK (subject_kind IN ('order', 'fill')),
    subject_id      BIGINT NOT NULL,
    strategy_id     UUID,                            -- null means taken out of any strategy
    admin_id        UUID,                            -- null when a rule did it
    rule_id         BIGINT,
    reason          VARCHAR,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX assignment_events_subject ON assignment_events (subject_kind, subject_id);

-- The current answer, written in the same transaction as the event.
CREATE TABLE current_assignments (
    subject_kind    VARCHAR NOT NULL CHECK (subject_kind IN ('order', 'fill')),
    subject_id      BIGINT NOT NULL,
    strategy_id     UUID NOT NULL,
    event_id        BIGINT NOT NULL,
    PRIMARY KEY (subject_kind, subject_id)
);
CREATE INDEX current_assignments_strategy ON current_assignments (strategy_id);

-- "All NIFTY weekly orders in account X go to strategy S until changed."
CREATE TABLE assignment_rules (
    id              BIGINT PRIMARY KEY DEFAULT nextval('assignment_rules_id_seq'),
    client_id       UUID NOT NULL,
    account_id      UUID,
    match           VARCHAR NOT NULL,                -- JSON text: underlying, expiry, product, side ...
    strategy_id     UUID NOT NULL,
    priority        INTEGER NOT NULL DEFAULT 100,
    enabled         BOOLEAN NOT NULL DEFAULT TRUE,
    created_by      UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Derived, rebuildable: strategy lots and how fills closed them.
CREATE TABLE lots (
    id              BIGINT PRIMARY KEY DEFAULT nextval('lots_id_seq'),
    strategy_id     UUID NOT NULL,
    account_id      UUID NOT NULL,
    segment         VARCHAR NOT NULL,
    token           VARCHAR NOT NULL,
    product         VARCHAR NOT NULL,
    open_fill_id    BIGINT NOT NULL,
    signed_quantity BIGINT NOT NULL,                 -- original, negative for a short
    remaining       BIGINT NOT NULL,                 -- signed, moves toward zero as it closes
    entry_price     DECIMAL(18, 4) NOT NULL,
    opened_at       TIMESTAMPTZ
);
CREATE INDEX lots_strategy ON lots (strategy_id);
CREATE INDEX lots_instrument ON lots (account_id, segment, token, product);

CREATE TABLE lot_closures (
    id                  BIGINT PRIMARY KEY DEFAULT nextval('lot_closures_id_seq'),
    lot_id              BIGINT NOT NULL,
    close_fill_id       BIGINT,
    position_event_id   BIGINT,
    quantity            BIGINT NOT NULL,             -- units closed
    exit_price          DECIMAL(18, 4) NOT NULL,
    realised_pnl        DECIMAL(18, 4) NOT NULL,
    closed_at           TIMESTAMPTZ
);
CREATE INDEX lot_closures_lot ON lot_closures (lot_id);

-- ------------------------------------------------------------- snapshots

-- One row per time an account's positions were read, even when it had none, so
-- "the latest read" is well defined and an empty book is not mistaken for an old one.
CREATE TABLE position_runs (
    id              BIGINT PRIMARY KEY DEFAULT nextval('position_runs_id_seq'),
    account_id      UUID NOT NULL,
    taken_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    row_count       INTEGER NOT NULL
);
CREATE INDEX position_runs_account ON position_runs (account_id, id);

CREATE TABLE position_snapshots (
    id              BIGINT PRIMARY KEY DEFAULT nextval('position_snapshots_id_seq'),
    run_id          BIGINT NOT NULL,
    account_id      UUID NOT NULL,
    taken_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    segment         VARCHAR,
    token           VARCHAR,
    trading_symbol  VARCHAR NOT NULL,
    product         VARCHAR NOT NULL,
    net_quantity    BIGINT NOT NULL,                 -- signed units
    average_price   DECIMAL(18, 4),
    last_price      DECIMAL(18, 4),
    pnl             DECIMAL(18, 4),
    raw             VARCHAR
);
CREATE INDEX position_snapshots_run ON position_snapshots (run_id);

CREATE TABLE margin_snapshots (
    id              BIGINT PRIMARY KEY DEFAULT nextval('margin_snapshots_id_seq'),
    account_id      UUID NOT NULL,
    taken_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    available       DECIMAL(18, 4) NOT NULL,         -- collateral included, as the broker's app shows it
    used            DECIMAL(18, 4) NOT NULL,
    total           DECIMAL(18, 4) NOT NULL,
    cash            DECIMAL(18, 4),
    collateral      DECIMAL(18, 4),
    raw             VARCHAR
);
CREATE INDEX margin_snapshots_account_time ON margin_snapshots (account_id, taken_at);

-- The broker's own ledger, imported from its back-office statement. This is a ledger;
-- a margin read is only a funds snapshot.
CREATE TABLE ledger_entries (
    id              BIGINT PRIMARY KEY DEFAULT nextval('ledger_entries_id_seq'),
    account_id      UUID NOT NULL,
    entry_date      DATE NOT NULL,
    voucher         VARCHAR,
    narration       VARCHAR,
    debit           DECIMAL(18, 4) NOT NULL DEFAULT 0,
    credit          DECIMAL(18, 4) NOT NULL DEFAULT 0,
    balance         DECIMAL(18, 4),
    source          VARCHAR NOT NULL CHECK (source IN ('import', 'api')),
    dedupe_key      VARCHAR NOT NULL,
    raw             VARCHAR,
    UNIQUE (account_id, dedupe_key)
);
CREATE INDEX ledger_entries_account_date ON ledger_entries (account_id, entry_date);

-- ----------------------------------------------------- watching the watchers

-- Quantity differences between the broker and our record.
CREATE TABLE recon_flags (
    id              BIGINT PRIMARY KEY DEFAULT nextval('recon_flags_id_seq'),
    account_id      UUID NOT NULL,
    segment         VARCHAR NOT NULL,
    token           VARCHAR NOT NULL,
    product         VARCHAR NOT NULL,
    broker_quantity BIGINT NOT NULL,
    our_quantity    BIGINT NOT NULL,
    detected_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at     TIMESTAMPTZ,
    note            VARCHAR
);

CREATE TABLE collector_state (
    account_id          UUID PRIMARY KEY,
    last_poll_at        TIMESTAMPTZ,
    last_ok_at          TIMESTAMPTZ,
    last_error          VARCHAR,
    stream_connected    BOOLEAN NOT NULL DEFAULT FALSE,
    session_expires_at  TIMESTAMPTZ
);

-- Everything an admin or the system did. Append-only.
CREATE TABLE audit_log (
    id              BIGINT PRIMARY KEY DEFAULT nextval('audit_log_id_seq'),
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    actor_kind      VARCHAR NOT NULL CHECK (actor_kind IN ('admin', 'system')),
    actor_id        UUID,
    action          VARCHAR NOT NULL,
    subject         VARCHAR,
    detail          VARCHAR
);
CREATE INDEX audit_log_time ON audit_log (occurred_at);

-- Retention settings, one row. 0 means "do not delete".
CREATE TABLE settings (
    key             VARCHAR PRIMARY KEY,
    value           VARCHAR NOT NULL
);
INSERT INTO settings VALUES ('heavy_data_days_after_expiry', '0');
