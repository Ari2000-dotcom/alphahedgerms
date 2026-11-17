-- Strategy tags on open positions.
--
-- A position is a leg of a strategy. The admin groups legs under a named strategy; the tag is keyed
-- by the position's identity (account, symbol, product), not by any one day's snapshot, so it
-- survives the brokers' daily resets and a position that closes and reopens keeps its strategy.
-- A position with no row here is "unassigned": the screen flags it so it can be sent to a strategy.
-- `strategies` (client_id, tag, name) already exists from 0001; a strategy belongs to one client.

CREATE TABLE position_tags (
    account_id      UUID NOT NULL,
    trading_symbol  VARCHAR NOT NULL,
    product         VARCHAR NOT NULL,
    strategy_id     UUID NOT NULL,
    tagged_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (account_id, trading_symbol, product)
);
CREATE INDEX position_tags_strategy ON position_tags (strategy_id);
