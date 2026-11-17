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
  strategy_id: null as string | null,
});

type Pos = ReturnType<typeof pos>;
const db = {
  strategies: [] as { id: string; client_id: string; tag: string; name: string; status: string; created_at: string; legs: number }[],
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
    pos("a4", "c3", "Kiran Bhatt", "zerodha", "KB5521", "NIFTY26DEC23500PE", -150, 212.4, 198.15),
    pos("a4", "c3", "Kiran Bhatt", "zerodha", "KB5521", "NIFTY2611324000PE", 150, 18.35, 22.8),
    pos("a4", "c3", "Kiran Bhatt", "zerodha", "KB5521", "NIFTY26DEC24500CE", -150, 164.75, 171.2),
  ],
};

const json = (o: unknown, status = 200) => new Response(JSON.stringify(o), { status, headers: { "content-type": "application/json" } });

window.fetch = async (url, init) => {
  await new Promise((r) => setTimeout(r, 80));
  const p = String(url).replace(/^\/api/, "");
  if (p === "/me") return json({ signed_in: true });
  if (p === "/overview") return json(db);
  if (p === "/refresh") return json({ ok: true });
  if (p === "/forgot") return json({ ok: true, minutes: 10, file: "D:\Alphahedge-RMS\data\reset-code.txt" });
  if (p === "/reset") return json(JSON.parse(String(init?.body)).code === "00000000" ? { error: "that reset code is wrong" } : { ok: true }, JSON.parse(String(init?.body)).code === "00000000" ? 403 : 200);
  if (p === "/password") { const b = JSON.parse(String(init?.body)); return b.current === "wrong" ? json({ error: "the current password is wrong" }, 403) : json({ ok: true }); }
  const body = init?.body ? JSON.parse(String(init.body)) : {};
  const recount = () => db.strategies.forEach((s) => (s.legs = (db.positions as Pos[]).filter((x) => x.strategy_id === s.id).length));
  const same = (x: Pos, l: { account_id: string; trading_symbol: string; product: string }) => x.account_id === l.account_id && x.trading_symbol === l.trading_symbol && x.product === l.product;
  if (p === "/strategies/tag") {
    let s = db.strategies.find((x) => x.id === body.strategy_id);
    if (!s) {
      if ([...db.strategies].some((x) => x.client_id === body.client_id && x.name.toLowerCase() === String(body.new_name).trim().toLowerCase())) return json({ error: `this client already has a strategy called ${body.new_name}` }, 409);
      const n = db.strategies.filter((x) => x.client_id === body.client_id).length + 1;
      s = { id: `s${Date.now()}`, client_id: body.client_id, tag: `S${n}`, name: String(body.new_name).trim(), status: "open", created_at: now, legs: 0 };
      db.strategies.push(s);
    }
    for (const l of body.legs) (db.positions as Pos[]).filter((x) => same(x, l)).forEach((x) => (x.strategy_id = s!.id));
    recount();
    return json({ strategy: s, tagged: body.legs.length });
  }
  if (p === "/strategies/untag") {
    for (const l of body.legs) (db.positions as Pos[]).filter((x) => same(x, l)).forEach((x) => (x.strategy_id = null));
    recount();
    return json({ untagged: body.legs.length });
  }
  const m = /^\/strategies\/(.+)$/.exec(p);
  if (m && init?.method === "PUT") { const s = db.strategies.find((x) => x.id === m[1]); if (s) s.name = body.name; return json({ ok: true }); }
  if (m && init?.method === "DELETE") { (db.positions as Pos[]).forEach((x) => { if (x.strategy_id === m[1]) x.strategy_id = null; }); db.strategies = db.strategies.filter((x) => x.id !== m[1]); return json({ ok: true }); }
  return json({ error: `not mocked: ${init?.method ?? "GET"} ${p}` }, 404);
};

export {};
