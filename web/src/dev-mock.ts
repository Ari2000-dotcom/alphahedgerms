// Development only: open the Vite dev server with ?mock to see every screen with sample data and no server.

const now = new Date().toISOString();
const margin = (available: number, used: number, cash: number | null, collateral: number | null) => ({ available, used, total: available + used, cash, collateral, taken_at: now });
const ok = { signed_in: true, error: null, margin_error: null, at: now };

const acc = (id: string, cid: string, name: string, code: string, broker: string, uid: string, saved: Record<string, boolean>, fields: Record<string, string>, status: unknown, m: unknown) => ({
  account: { id, client_id: cid, client_code: code, client_name: name, broker, broker_user_id: uid, label: `${broker} ${uid}`, enabled: true, created_at: now },
  status, margin: m, saved, fields,
});

const client = (id: string, code: string, name: string, accounts: number) => ({ id, client_code: code, name, email: null, phone: null, notes: null, status: "active", created_at: now, accounts });

const pos = (account_id: string, client_id: string, client_name: string, broker: string, uid: string, trading_symbol: string, net_quantity: number, average_price: number, last_price: number, product = "NRML") => ({
  account_id, client_id, client_name, broker, account_label: `${broker} ${uid}`, taken_at: now, trading_symbol, segment: "NFO", token: null, product, net_quantity, average_price, last_price,
  pnl: Math.round((last_price - average_price) * net_quantity * 100) / 100,
});

const db = {
  clients: [client("c1", "C001", "Ravi Menon", 2), client("c2", "C002", "Asha Rao", 1), client("c3", "C003", "Kiran Bhatt", 1)],
  accounts: [
    acc("a1", "c1", "Ravi Menon", "C001", "zerodha", "AB1234", { apiKey: true, apiSecret: true, password: true, totpSecret: true }, {}, ok, margin(5448697.49, 351302.51, 85086.5, 5363610.99)),
    acc("a2", "c1", "Ravi Menon", "C001", "kotak", "K77AB", { apiKey: true, pin: true, totpSecret: true }, { mobile: "+919800000001" }, { ...ok, margin_error: "Kotak's margin service is not answering right now (HTTP 424). It clears on its own." }, null),
    acc("a3", "c2", "Asha Rao", "C002", "angel", "A123456", { apiKey: true, pin: true, totpSecret: true }, {}, { signed_in: false, error: "Angel: Invalid totp [AB1050]", margin_error: null, at: now }, null),
    acc("a4", "c3", "Kiran Bhatt", "C003", "zerodha", "KB5521", { apiKey: true, apiSecret: true, password: true, totpSecret: true }, {}, ok, margin(1204550.2, 795449.8, 1204550.2, 0)),
  ],
  positions: [
    pos("a1", "c1", "Ravi Menon", "zerodha", "AB1234", "NIFTY2611324500CE", -75, 101.25, 88.4),
    pos("a1", "c1", "Ravi Menon", "zerodha", "AB1234", "NIFTY2611324500PE", -75, 96.1, 112.35),
    pos("a1", "c1", "Ravi Menon", "zerodha", "AB1234", "BANKNIFTY26NOVFUT", 30, 52118.4, 52340.0, "MIS"),
    pos("a4", "c3", "Kiran Bhatt", "zerodha", "KB5521", "NIFTY2611325000CE", 150, 41.55, 48.9),
  ],
};

const json = (o: unknown, status = 200) => new Response(JSON.stringify(o), { status, headers: { "content-type": "application/json" } });

window.fetch = async (url, init) => {
  await new Promise((r) => setTimeout(r, 80));
  const p = String(url).replace(/^\/api/, "");
  if (p === "/me") return json({ signed_in: true });
  if (p === "/overview") return json(db);
  if (p === "/refresh") return json({ ok: true });
  return json({ error: `not mocked: ${init?.method ?? "GET"} ${p}` }, 404);
};

export {};
