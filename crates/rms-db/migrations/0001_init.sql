-- Alphahedge RMS: initial schema.
--
-- Rules the whole schema follows:
--   * Money and prices are NUMERIC, never floating point.
--   * Quantities are in units (not lots), as BIGINT. A signed quantity is
--     positive for long and negative for short.
--   * Tables that are a record of what happened are append-only: a trigger
--     refuses UPDATE and DELETE. A correction is a new row.
--   * Every row that belongs to a client carries client_id, so a client portal
--     can be added later with row-level security and no schema change.
--   * Tables marked "derived" are caches that can be rebuilt from the record.

-- ---------------------------------------------------------------- people

CREATE TABLE admins (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email           TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    password_hash   TEXT NOT NULL,
    totp_sealed     BYTEA,
    role            TEXT NOT NULL DEFAULT 'admin' CHECK (role IN ('admin', 'risk_readonly')),
    disabled        BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- A client is the owner of one or more broker accounts: the "user" of the later portal.
CREATE TABLE clients (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    client_code     TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    email           TEXT,
    phone           TEXT,
    status          TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'inactive')),
    notes           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE broker_accounts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    client_id       UUID NOT NULL REFERENCES clients(id),
    broker          TEXT NOT NULL CHECK (broker IN ('zerodha', 'angel', 'kotak', 'upstox', 'nubra')),
    broker_user_id  TEXT NOT NULL,                 -- client id / UCC at the broker
    label           TEXT NOT NULL,
    credentials     BYTEA NOT NULL,                -- sealed with the vault, never plain
    enabled         BOOLEAN NOT NULL DEFAULT TRUE,
    created_by      UUID REFERENCES admins(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (broker, broker_user_id)
);
CREATE INDEX broker_accounts_client ON broker_accounts (client_id);

-- ------------------------------------------------------------ instruments

-- Keyed by the exchange token, which is the same at every broker and is also
-- the key the hosted market feed uses (quote:NSE_FO:43721).
CREATE TABLE instruments (
    segment         TEXT NOT NULL,                 -- NSE_EQ, NSE_FO, BSE_FO, MCX_FO ...
    token           TEXT NOT NULL,
    trading_symbol  TEXT NOT NULL,
    name            TEXT,
    underlying      TEXT,
    instrument_type TEXT NOT NULL CHECK (instrument_type IN ('EQ', 'FUT', 'CE', 'PE', 'INDEX', 'OTHER')),
    expiry          DATE,
    strike          NUMERIC(18, 4),
    lot_size        INTEGER,
    tick_size       NUMERIC(18, 4),
    updated_on      DATE NOT NULL DEFAULT CURRENT_DATE,
    PRIMARY KEY (segment, token)
);
CREATE INDEX instruments_symbol ON instruments (trading_symbol);
CREATE INDEX instruments_underlying_expiry ON instruments (underlying, expiry);

-- Each broker's own symbol for an instrument.
CREATE TABLE instrument_aliases (
    broker          TEXT NOT NULL,
    broker_symbol   TEXT NOT NULL,
    segment         TEXT NOT NULL,
    token           TEXT NOT NULL,
    PRIMARY KEY (broker, broker_symbol),
    FOREIGN KEY (segment, token) REFERENCES instruments (segment, token)
);

-- ---------------------------------------------------------------- orders

-- Current state of each order (updated as events arrive).
CREATE TABLE orders (
    id                  BIGSERIAL PRIMARY KEY,
    account_id          UUID NOT NULL REFERENCES broker_accounts(id),
    client_id           UUID NOT NULL REFERENCES clients(id),
    broker_order_id     TEXT NOT NULL,
    exchange_order_id   TEXT,
    parent_order_id     BIGINT REFERENCES orders(id),   -- child orders of a bracket or cover
    segment             TEXT,
    token               TEXT,
    trading_symbol      TEXT NOT NULL,
    side                TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
    product             TEXT NOT NULL,                  -- MIS, NRML, CNC ...
    order_type          TEXT NOT NULL,
    quantity            BIGINT NOT NULL,
    price               NUMERIC(18, 4),
    trigger_price       NUMERIC(18, 4),
    status              TEXT NOT NULL,
    filled_quantity     BIGINT NOT NULL DEFAULT 0,
    average_price       NUMERIC(18, 4),
    tag                 TEXT,
    placed_at           TIMESTAMPTZ,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    raw                 JSONB NOT NULL,
    UNIQUE (account_id, broker_order_id)
);
CREATE INDEX orders_client_day ON orders (client_id, placed_at);
CREATE INDEX orders_instrument ON orders (segment, token);

-- Every state an order went through, including rejections and cancellations.
-- These never touch strategy lots. Append-only.
CREATE TABLE order_events (
    id                  BIGSERIAL PRIMARY KEY,
    account_id          UUID NOT NULL REFERENCES broker_accounts(id),
    broker_order_id     TEXT NOT NULL,
    status              TEXT NOT NULL,
    filled_quantity     BIGINT,
    average_price       NUMERIC(18, 4),
    reason              TEXT,
    event_key           TEXT NOT NULL,                  -- broker-specific, makes the insert idempotent
    observed_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    source              TEXT NOT NULL CHECK (source IN ('live', 'poll', 'sweep', 'import')),
    raw                 JSONB NOT NULL,
    UNIQUE (account_id, event_key)
);
CREATE INDEX order_events_order ON order_events (account_id, broker_order_id);

-- ----------------------------------------------------------------- fills

-- One row per trade. The source of truth for quantity and price. Append-only.
CREATE TABLE fills (
    id                  BIGSERIAL PRIMARY KEY,           -- also the ingest sequence for tie-breaks
    account_id          UUID NOT NULL REFERENCES broker_accounts(id),
    client_id           UUID NOT NULL REFERENCES clients(id),
    broker_order_id     TEXT NOT NULL,
    exchange            TEXT NOT NULL,
    exchange_order_id   TEXT,
    exchange_trade_id   TEXT,
    -- Natural key. Where a broker gives no trade id (Kotak) the collector builds
    -- it from order number, exchange order id, exchange time, quantity and price.
    dedupe_key          TEXT NOT NULL,
    segment             TEXT,
    token               TEXT,
    trading_symbol      TEXT NOT NULL,
    side                TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
    product             TEXT NOT NULL,
    quantity            BIGINT NOT NULL CHECK (quantity > 0),
    price               NUMERIC(18, 4) NOT NULL,
    exchange_ts         TIMESTAMPTZ,                     -- preferred; some brokers lack seconds
    broker_ts           TIMESTAMPTZ,
    source              TEXT NOT NULL CHECK (source IN ('live', 'sweep', 'import')),
    raw                 JSONB NOT NULL,
    ingested_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (account_id, dedupe_key)
);
CREATE INDEX fills_order ON fills (account_id, broker_order_id);
CREATE INDEX fills_instrument_time ON fills (account_id, segment, token, exchange_ts);
CREATE INDEX fills_client_time ON fills (client_id, exchange_ts);

-- Position changes that are not trades. Without these every expiry, conversion
-- and imported opening position would look like a reconciliation error.
CREATE TABLE position_events (
    id              BIGSERIAL PRIMARY KEY,
    account_id      UUID NOT NULL REFERENCES broker_accounts(id),
    client_id       UUID NOT NULL REFERENCES clients(id),
    kind            TEXT NOT NULL CHECK (kind IN ('conversion', 'expiry_settle', 'exercise', 'delivery', 'import_open', 'manual_adjust')),
    segment         TEXT NOT NULL,
    token           TEXT NOT NULL,
    product         TEXT NOT NULL,
    quantity_delta  BIGINT NOT NULL,                     -- signed units
    price           NUMERIC(18, 4),                      -- settlement or opening price
    effective_at    TIMESTAMPTZ NOT NULL,
    note            TEXT,
    created_by      UUID REFERENCES admins(id),          -- null when made by the system
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX position_events_instrument ON position_events (account_id, segment, token);

-- ------------------------------------------------------------ strategies

CREATE TABLE strategies (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    client_id       UUID NOT NULL REFERENCES clients(id),
    tag             TEXT NOT NULL,
    name            TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
    notes           TEXT,
    created_by      UUID REFERENCES admins(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at       TIMESTAMPTZ,
    UNIQUE (client_id, tag)
);

-- Every tagging, re-tagging and un-tagging, with who and why. Append-only.
CREATE TABLE assignment_events (
    id              BIGSERIAL PRIMARY KEY,
    client_id       UUID NOT NULL REFERENCES clients(id),
    subject_kind    TEXT NOT NULL CHECK (subject_kind IN ('order', 'fill')),
    subject_id      BIGINT NOT NULL,
    strategy_id     UUID REFERENCES strategies(id),      -- null means taken out of any strategy
    admin_id        UUID REFERENCES admins(id),          -- null when a rule did it
    rule_id         BIGINT,
    reason          TEXT,
    at              TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX assignment_events_subject ON assignment_events (subject_kind, subject_id);

-- The current answer, written in the same transaction as the event.
CREATE TABLE current_assignments (
    subject_kind    TEXT NOT NULL CHECK (subject_kind IN ('order', 'fill')),
    subject_id      BIGINT NOT NULL,
    strategy_id     UUID NOT NULL REFERENCES strategies(id),
    event_id        BIGINT NOT NULL REFERENCES assignment_events(id),
    PRIMARY KEY (subject_kind, subject_id)
);
CREATE INDEX current_assignments_strategy ON current_assignments (strategy_id);

-- "All NIFTY weekly orders in account X go to strategy S until changed."
CREATE TABLE assignment_rules (
    id              BIGSERIAL PRIMARY KEY,
    client_id       UUID NOT NULL REFERENCES clients(id),
    account_id      UUID REFERENCES broker_accounts(id),
    match           JSONB NOT NULL,                      -- underlying, expiry, product, side ...
    strategy_id     UUID NOT NULL REFERENCES strategies(id),
    priority        INTEGER NOT NULL DEFAULT 100,
    enabled         BOOLEAN NOT NULL DEFAULT TRUE,
    created_by      UUID REFERENCES admins(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Derived, rebuildable: strategy lots and how fills closed them.
CREATE TABLE lots (
    id              BIGSERIAL PRIMARY KEY,
    strategy_id     UUID NOT NULL REFERENCES strategies(id),
    account_id      UUID NOT NULL REFERENCES broker_accounts(id),
    segment         TEXT NOT NULL,
    token           TEXT NOT NULL,
    product         TEXT NOT NULL,
    open_fill_id    BIGINT NOT NULL REFERENCES fills(id),
    signed_quantity BIGINT NOT NULL,                     -- original, negative for a short
    remaining       BIGINT NOT NULL,                     -- signed, moves toward zero as it closes
    entry_price     NUMERIC(18, 4) NOT NULL,
    opened_at       TIMESTAMPTZ
);
CREATE INDEX lots_strategy ON lots (strategy_id);
CREATE INDEX lots_open ON lots (account_id, segment, token, product) WHERE remaining <> 0;

CREATE TABLE lot_closures (
    id              BIGSERIAL PRIMARY KEY,
    lot_id          BIGINT NOT NULL REFERENCES lots(id) ON DELETE CASCADE,
    close_fill_id   BIGINT REFERENCES fills(id),
    position_event_id BIGINT REFERENCES position_events(id),
    quantity        BIGINT NOT NULL,                     -- units closed
    exit_price      NUMERIC(18, 4) NOT NULL,
    realised_pnl    NUMERIC(18, 4) NOT NULL,
    closed_at       TIMESTAMPTZ
);
CREATE INDEX lot_closures_lot ON lot_closures (lot_id);

-- ------------------------------------------------------------- snapshots

CREATE TABLE position_snapshots (
    id              BIGSERIAL PRIMARY KEY,
    account_id      UUID NOT NULL REFERENCES broker_accounts(id),
    taken_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    segment         TEXT,
    token           TEXT,
    trading_symbol  TEXT NOT NULL,
    product         TEXT NOT NULL,
    net_quantity    BIGINT NOT NULL,                     -- signed units
    average_price   NUMERIC(18, 4),
    last_price      NUMERIC(18, 4),
    pnl             NUMERIC(18, 4),
    raw             JSONB NOT NULL
);
CREATE INDEX position_snapshots_account_time ON position_snapshots (account_id, taken_at DESC);

CREATE TABLE margin_snapshots (
    id              BIGSERIAL PRIMARY KEY,
    account_id      UUID NOT NULL REFERENCES broker_accounts(id),
    taken_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    available       NUMERIC(18, 4) NOT NULL,             -- collateral included, as the broker's app shows it
    used            NUMERIC(18, 4) NOT NULL,
    total           NUMERIC(18, 4) NOT NULL,
    cash            NUMERIC(18, 4),
    collateral      NUMERIC(18, 4),
    raw             JSONB NOT NULL
);
CREATE INDEX margin_snapshots_account_time ON margin_snapshots (account_id, taken_at DESC);

-- The broker's own ledger, imported from its back-office statement. This is a
-- ledger; a margin read is only a funds snapshot.
CREATE TABLE ledger_entries (
    id              BIGSERIAL PRIMARY KEY,
    account_id      UUID NOT NULL REFERENCES broker_accounts(id),
    entry_date      DATE NOT NULL,
    voucher         TEXT,
    narration       TEXT,
    debit           NUMERIC(18, 4) NOT NULL DEFAULT 0,
    credit          NUMERIC(18, 4) NOT NULL DEFAULT 0,
    balance         NUMERIC(18, 4),
    source          TEXT NOT NULL CHECK (source IN ('import', 'api')),
    dedupe_key      TEXT NOT NULL,
    raw             JSONB,
    UNIQUE (account_id, dedupe_key)
);
CREATE INDEX ledger_entries_account_date ON ledger_entries (account_id, entry_date);

-- ----------------------------------------------------- watching the watchers

-- Quantity differences between the broker and our record.
CREATE TABLE recon_flags (
    id              BIGSERIAL PRIMARY KEY,
    account_id      UUID NOT NULL REFERENCES broker_accounts(id),
    segment         TEXT NOT NULL,
    token           TEXT NOT NULL,
    product         TEXT NOT NULL,
    broker_quantity BIGINT NOT NULL,
    our_quantity    BIGINT NOT NULL,
    detected_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at     TIMESTAMPTZ,
    note            TEXT
);
CREATE INDEX recon_flags_open ON recon_flags (account_id) WHERE resolved_at IS NULL;

CREATE TABLE collector_state (
    account_id          UUID PRIMARY KEY REFERENCES broker_accounts(id),
    last_poll_at        TIMESTAMPTZ,
    last_ok_at          TIMESTAMPTZ,
    last_error          TEXT,
    stream_connected    BOOLEAN NOT NULL DEFAULT FALSE,
    session_expires_at  TIMESTAMPTZ
);

-- Everything an admin or the system did. Append-only.
CREATE TABLE audit_log (
    id              BIGSERIAL PRIMARY KEY,
    at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    actor_kind      TEXT NOT NULL CHECK (actor_kind IN ('admin', 'system')),
    actor_id        UUID,
    action          TEXT NOT NULL,
    subject         TEXT,
    detail          JSONB
);
CREATE INDEX audit_log_time ON audit_log (at DESC);

-- ------------------------------------------------------------ append-only

CREATE FUNCTION refuse_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '% is append-only: % is not allowed (write a new row instead)', TG_TABLE_NAME, TG_OP
        USING ERRCODE = 'restrict_violation';
END;
$$;

CREATE TRIGGER fills_append_only BEFORE UPDATE OR DELETE ON fills FOR EACH ROW EXECUTE FUNCTION refuse_change();
CREATE TRIGGER order_events_append_only BEFORE UPDATE OR DELETE ON order_events FOR EACH ROW EXECUTE FUNCTION refuse_change();
CREATE TRIGGER position_events_append_only BEFORE UPDATE OR DELETE ON position_events FOR EACH ROW EXECUTE FUNCTION refuse_change();
CREATE TRIGGER assignment_events_append_only BEFORE UPDATE OR DELETE ON assignment_events FOR EACH ROW EXECUTE FUNCTION refuse_change();
CREATE TRIGGER audit_log_append_only BEFORE UPDATE OR DELETE ON audit_log FOR EACH ROW EXECUTE FUNCTION refuse_change();
