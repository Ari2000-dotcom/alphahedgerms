import { useEffect, useState } from "preact/hooks";
import { signal } from "@preact/signals";
import { api, ApiError, overview, signedIn, type AccountView, type Client, type Position } from "./api";
import { BROKER_ORDER, BROKERS, type BrokerId } from "./brokers";
import { fail, go, Money, say, toast } from "./ui";
import { ClientsGrid, startAdding } from "./clients";
import * as I from "./icons";
import { PositionsTable } from "./positions";
import { ForgotPanel, PasswordSheet, passwordOpen } from "./account";

// -------------------------------------------------------------------- state

const route = signal(location.hash || "#/");
window.addEventListener("hashchange", () => (route.value = location.hash || "#/"));

const busy = signal(false);

async function readBrokers() {
  if (busy.value) return;
  busy.value = true;
  try {
    await api.refreshAll();
  } catch (e) {
    fail(e);
  } finally {
    busy.value = false;
  }
}

// ------------------------------------------------------------------ sign in

function Login() {
  const [pw, setPw] = useState("");
  const [show, setShow] = useState(false);
  const [err, setErr] = useState("");
  const [wait, setWait] = useState(false);
  const [forgot, setForgot] = useState(false);
  async function submit(e: Event) {
    e.preventDefault();
    if (!pw || wait) return;
    setWait(true);
    setErr("");
    try {
      await api.login(pw);
      await api.loadOverview();
    } catch (x) {
      setErr(x instanceof ApiError ? x.message : String(x));
    } finally {
      setWait(false);
    }
  }
  if (forgot) {
    return (
      <div class="login">
        <div class="logincard"><ForgotPanel onBack={() => setForgot(false)} /></div>
      </div>
    );
  }
  return (
    <div class="login">
      <form class="logincard" onSubmit={submit}>
        <div class="loginhead">
          <span class="mark big"><I.Mark /></span>
          <h1>Alphahedge RMS</h1>
          <p class="sub">Sign in to the admin console</p>
        </div>
        <label class="fld">
          <span class="lab">Password</span>
          <span class="inputwrap" data-invalid={err ? "true" : undefined}>
            <I.Lock />
            {show
              ? <input type="text" autocomplete="off" spellcheck={false} value={pw} autofocus onInput={(e) => setPw(e.currentTarget.value)} aria-invalid={err ? "true" : undefined} />
              : <input type="password" autocomplete="current-password" value={pw} autofocus onInput={(e) => setPw(e.currentTarget.value)} aria-invalid={err ? "true" : undefined} />}
            <button type="button" class="ic reveal" onClick={() => setShow(!show)} aria-label={show ? "Hide password" : "Show password"} aria-pressed={show}>{show ? <I.EyeOff /> : <I.Eye />}</button>
          </span>
        </label>
        {err && <p class="banner" role="alert">{err}</p>}
        <button class="btn primary lg" type="submit" disabled={wait || !pw}>{wait ? <><span class="spin"><I.Refresh /></span> Checking…</> : <>Sign in <I.ArrowNext /></>}</button>
        <button type="button" class="linkbtn forgotlink" onClick={() => setForgot(true)}>Forgot password?</button>
        <p class="loginnote">Administrator access only. Sessions end when you sign out.</p>
      </form>
    </div>
  );
}

// ------------------------------------------------------------------- shell

const NAV: { label: string; to: string; icon: (p: { on?: boolean }) => preact.JSX.Element; live: boolean }[] = [
  { label: "Overview", to: "#/", icon: I.NavOverview, live: true },
  { label: "Clients", to: "#/clients", icon: I.NavAccounts, live: true },
  { label: "Positions", to: "#/positions", icon: I.NavPositions, live: true },
  { label: "Orders and trades", to: "", icon: I.NavOrders, live: false },
  { label: "Strategies", to: "", icon: I.NavTrade, live: false },
  { label: "Risk limits", to: "", icon: I.NavRisk, live: false },
];

const RAIL_KEY = "rms.nav.rail";
const rail = signal(((): boolean => { try { return localStorage.getItem(RAIL_KEY) === "1"; } catch { return false; } })());
function toggleRail() {
  rail.value = !rail.value;
  try { localStorage.setItem(RAIL_KEY, rail.value ? "1" : "0"); } catch { /* storage blocked: the choice lasts this visit only */ }
}

function Shell({ children }: { children: preact.ComponentChildren }) {
  const r = route.value;
  const active = (to: string) => (to === "#/" ? r === "#/" || r === "" : r.startsWith(to));
  const last = overview.value?.accounts.map((a) => a.status?.at).filter(Boolean).sort().pop();
  // The read gets old quietly (a closed laptop, a stopped server): after 3 minutes the bar says so, in amber.
  const [, tick] = useState(0);
  useEffect(() => {
    const t = window.setInterval(() => tick((n) => n + 1), 30000);
    return () => clearInterval(t);
  }, []);
  const ageMin = last ? Math.floor((Date.now() - new Date(last).getTime()) / 60000) : 0;
  const stale = !!last && ageMin >= 3 && !busy.value;
  const page = NAV.find((n) => n.live && n.to !== "" && active(n.to))?.label ?? (r.startsWith("#/clients/") ? "Client" : "Overview");
  useEffect(() => { document.title = `${page} · Alphahedge RMS`; }, [page]);
  return (
    <div class={"shell" + (rail.value ? " rail" : "")}>
      <header class="top">
        <button class="ic navtoggle" onClick={toggleRail} aria-label={rail.value ? "Show side bar" : "Hide side bar"} aria-expanded={!rail.value} title={rail.value ? "Show side bar" : "Hide side bar"}>
          <I.Sidebar />
        </button>
        <div class="brand">
          <span class="mark"><I.Mark /></span>
          <b>Alphahedge RMS</b>
        </div>
        <span class="crumb" aria-hidden="true"><i>/</i>{NAV.find((n) => n.live && active(n.to) && n.to !== "")?.label ?? "Client"}</span>
        <span class="grow" />
        <span class={"status" + (stale ? " stale" : "")} title={last ? `Last read ${new Date(last).toLocaleString("en-IN")}` : undefined}><i class={"dot" + (last && !stale ? " ok" : "")} />{last ? (stale ? `Last read ${ageMin >= 60 ? `${Math.floor(ageMin / 60)} h` : `${ageMin} min`} ago` : `Last read ${new Date(last).toLocaleTimeString("en-IN")}`) : "Not read from brokers yet"}</span>
        <button class="btn" disabled={busy.value} onClick={readBrokers} title="Sign in where needed and read every account's positions and margin now">
          <span class={busy.value ? "spin" : ""}><I.Refresh /></span>
          {busy.value ? "Reading…" : "Read brokers"}
        </button>
        <span class="vbar" aria-hidden="true" />
        <button class="btn quiet" onClick={() => api.logout().catch(fail)}><I.LogOut /> Sign out</button>
      </header>
      <div class="body">
        <nav class="nav" aria-label="Sections">
          <div class="navgroup">
            <span class="navhead">Workspace</span>
            {NAV.filter((n) => n.live).map((n) => (
              <a key={n.label} href={n.to} data-label={n.label} class={"navbtn" + (active(n.to) ? " on" : "")} aria-current={active(n.to) ? "page" : undefined}>
                <n.icon on={active(n.to)} />
                <span class="lbl">{n.label}</span>
              </a>
            ))}
          </div>
          <div class="navgroup later">
            <span class="navhead">Coming soon</span>
            {NAV.filter((n) => !n.live).map((n) => (
              <button key={n.label} data-label={n.label} class="navbtn soon" title={`${n.label} is not built yet`} onClick={() => say(`${n.label} is not built yet`)}>
                <n.icon on={active(n.to)} />
                <span class="lbl">{n.label}</span>
              </button>
            ))}
          </div>
          <button class="navfoot" onClick={() => (passwordOpen.value = true)} title="Change password" aria-label="Admin account: change password">
            <span class="avatar" aria-hidden="true">A</span>
            <span class="who2"><b>Admin</b><small>Change password</small></span>
            <span class="navkey" aria-hidden="true"><I.Key /></span>
          </button>
        </nav>
        <main>{children}</main>
      </div>
      <PasswordSheet />
      <div class={"toast" + (toast.value ? " show" : "")} role="status" aria-live="polite">{toast.value}</div>
    </div>
  );
}

// ------------------------------------------------------------------ figures

const FIG_ICON: Record<string, () => preact.JSX.Element> = {
  "Accounts signed in": I.FigSignedIn,
  "Available margin": I.FigAvailable,
  "Used margin": I.FigUsed,
  "Open positions": I.FigOpen,
  "Open P&L": I.FigPnl,
};

function Figures({ items }: { items: { label: string; value: preact.ComponentChildren; sub?: string }[] }) {
  return (
    <div class="figures">
      {items.map((f) => (
        <div class="fig" key={f.label}>
          <span class="figtop">
            <span class="lab">{f.label}</span>
            <span class="figicon">{FIG_ICON[f.label]?.()}</span>
          </span>
          <b class="val">{f.value}</b>
          {f.sub && <small class="figsub">{f.sub}</small>}
        </div>
      ))}
    </div>
  );
}

function totals(accounts: AccountView[], positions: Position[]) {
  const live = accounts.filter((a) => a.account.enabled);
  const sum = (pick: (m: NonNullable<AccountView["margin"]>) => number | null) => live.reduce((t, a) => t + (a.margin ? pick(a.margin) ?? 0 : 0), 0);
  return {
    accounts: live.length,
    signedIn: live.filter((a) => a.status?.signed_in).length,
    available: sum((m) => m.available),
    used: sum((m) => m.used),
    pnl: positions.reduce((t, p) => t + (p.pnl ?? 0), 0),
    open: positions.filter((p) => p.net_quantity !== 0).length,
  };
}

// ----------------------------------------------------------------- overview

function Overview() {
  const o = overview.value;
  if (!o) return <Loading />;
  const t = totals(o.accounts, o.positions);
  const trouble = o.accounts.filter((a) => a.account.enabled && a.status && (a.status.error || a.status.margin_error));
  if (o.clients.length === 0) {
    return (
      <section class="screen">
        <div class="canvas"><div class="empty">
          <span class="glyph"><I.NavAccounts /></span>
          <h2>Add your first client</h2>
          <p>Add a client with their broker's sign-in details in one row, the way the copy-trading desk adds an account. Their positions and margin then appear here.</p>
          <button class="btn primary" onClick={() => { startAdding(); go("#/clients"); }}><I.Plus /> Add a client</button>
        </div></div>
      </section>
    );
  }
  return (
    <section class="screen">
      <div class="strip"><h1>Overview</h1><span class="count">{o.clients.length} client{o.clients.length === 1 ? "" : "s"}</span></div>
      <Figures
        items={[
          { label: "Accounts signed in", value: <>{t.signedIn}<span class="of"> of {t.accounts}</span></> },
          { label: "Available margin", value: <Money n={t.available} /> },
          { label: "Used margin", value: <Money n={t.used} /> },
          { label: "Open positions", value: t.open },
          { label: "Open P&L", value: <Money n={t.pnl} plus tone /> },
        ]}
      />
      {trouble.length > 0 && (
        <div class="block">
          <div class="strip"><h2>Needs attention</h2><span class="count">{trouble.length}</span></div>
          <div class="canvas attention">
            {trouble.map((a) => (
              <div class="issue" key={a.account.id}>
                <span class="state"><i class="dot bad" /><a href={`#/clients/${a.account.client_id}`}>{a.account.client_name}</a> · {BROKERS[a.account.broker as BrokerId]?.name ?? a.account.broker} {a.account.broker_user_id}</span>
                <span class="why">{a.status?.error ?? a.status?.margin_error}</span>
              </div>
            ))}
          </div>
        </div>
      )}
      <PositionsTable rows={o.positions} empty="No open positions yet. Press Read brokers to fetch them." />
    </section>
  );
}

function Loading() {
  return <section class="screen"><div class="canvas"><div class="skel" aria-hidden="true">{[0, 1, 2].map((i) => <div key={i} />)}</div></div></section>;
}

// ------------------------------------------------------------ client detail

function AddAccount({ client, onDone }: { client: Client; onDone: () => void }) {
  const [broker, setBroker] = useState<BrokerId>("zerodha");
  const [vals, setVals] = useState<Record<string, string>>({});
  const [focus, setFocus] = useState("");
  const [err, setErr] = useState("");
  const [saving, setSaving] = useState(false);
  const def = BROKERS[broker];
  const filled = (k: string) => !!(vals[k] ?? "").trim();
  const next = def.fields.find((f) => !filled(f.key));
  const hint = def.fields.find((f) => f.key === focus) ?? next;

  async function save() {
    if (saving) return;
    const missing = def.fields.filter((f) => !filled(f.key));
    if (missing.length) return setErr(`${def.name} needs: ${missing.map((f) => f.label).join(", ")}`);
    setSaving(true);
    setErr("");
    try {
      const secrets: Record<string, string> = {};
      const fields: Record<string, string> = {};
      let user_id = "";
      for (const f of def.fields) {
        const v = vals[f.key].trim();
        if (f.key === "userId") user_id = v;
        else if (f.secret) secrets[f.key] = v;
        else fields[f.key] = v;
      }
      const r = await api.addAccount(client.id, { broker, user_id, secrets, fields });
      say(r.status.signed_in ? `${def.name} account added and signed in` : `Account saved, but sign-in failed: ${r.status.error ?? "unknown error"}`);
      onDone();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }
  const keys = (e: KeyboardEvent) => {
    if (e.key === "Enter") { e.preventDefault(); void save(); }
    if (e.key === "Escape") onDone();
  };
  const common = (f: (typeof def.fields)[number]) => ({
    value: vals[f.key] ?? "",
    autocomplete: "off",
    spellcheck: false,
    placeholder: f.label,
    "aria-invalid": err && !filled(f.key) ? ("true" as const) : undefined,
    autofocus: f === def.fields[0],
    onInput: (e: { currentTarget: HTMLInputElement }) => { const v = e.currentTarget.value; setVals((p) => ({ ...p, [f.key]: v })); },
    onFocus: () => setFocus(f.key),
    onKeyDown: keys,
  });

  return (
    <div class="addpanel">
      <h2>Add a broker account for {client.name}</h2>
      <fieldset class="brokers">
        <legend>Broker</legend>
        <div class="chips" role="radiogroup" aria-label="Broker">
          {BROKER_ORDER.map((b) => (
            <label key={b} class={"chip-opt" + (broker === b ? " on" : "")}>
              <input type="radio" name="broker" value={b} checked={broker === b} onChange={() => { setBroker(b); setVals({}); setErr(""); }} />
              {BROKERS[b].name}
            </label>
          ))}
        </div>
      </fieldset>
      <div class="addgrid">
        {def.fields.map((f) => (
          <label class="fld" key={broker + f.key}>
            <span class="lab">{f.label}{filled(f.key) && <em class="saved"> · filled</em>}</span>
            {f.secret ? <input type="password" {...common(f)} /> : <input type="text" {...common(f)} />}
          </label>
        ))}
      </div>
      <p class="hintline" role="status">
        {hint ? <><strong>{hint.label}.</strong> {hint.where}</> : "All filled in. Press Enter to save and sign in."}
        {def.note && <span class="note2"> {def.note}</span>}
      </p>
      {err && <p class="banner" role="alert">{err}</p>}
      <div class="addfoot">
        <button class="btn primary" disabled={saving} onClick={save}>{saving ? "Signing in…" : "Save and sign in"}</button>
        <button class="btn quiet" onClick={onDone}>Cancel</button>
      </div>
    </div>
  );
}

function AccountsTable({ accounts }: { accounts: AccountView[] }) {
  const [one, setOne] = useState<string | null>(null);
  async function refresh(id: string) {
    setOne(id);
    try { await api.refreshOne(id); } catch (e) { fail(e); } finally { setOne(null); }
  }
  async function remove(a: AccountView) {
    if (!confirm(`Delete ${a.account.label}? Its saved sign-in details are removed. Positions and orders at the broker are not touched.`)) return;
    try { await api.deleteAccount(a.account.id); say("Account deleted"); } catch (e) { fail(e); }
  }
  return (
    <div class="canvas">
      <table class="rtable">
        <thead>
          <tr>
            <th>Broker</th><th>Account</th><th>Status</th><th class="num">Available</th><th class="num">Used</th><th class="num">Total</th><th class="num">Cash</th><th class="num">Collateral</th><th>Active</th><th><span class="vh">Actions</span></th>
          </tr>
        </thead>
        <tbody>
          {accounts.map((a, i) => {
            const s = a.status;
            const m = a.margin;
            return (
              <>
                <tr class="rrow" key={a.account.id} style={{ "--i": Math.min(i, 14) } as never}>
                  <td class="name">{BROKERS[a.account.broker as BrokerId]?.name ?? a.account.broker}</td>
                  <td class="sub">{a.account.broker_user_id}</td>
                  <td>
                    {!s ? <span class="state"><i class="dot" />Not read yet</span> : s.signed_in ? <span class="state ok"><i class="dot ok" />Signed in</span> : <span class="state linklike"><i class="dot bad" />Sign-in failed</span>}
                  </td>
                  <td class="num"><Money n={m?.available} /></td>
                  <td class="num"><Money n={m?.used} /></td>
                  <td class="num"><Money n={m?.total} /></td>
                  <td class="num">{m && m.cash === null ? <span class="money zero" title="This broker does not report cash on its own">–</span> : <Money n={m?.cash} />}</td>
                  <td class="num"><Money n={m?.collateral} /></td>
                  <td><input type="checkbox" checked={a.account.enabled} aria-label={`${a.account.label} active`} onChange={(e) => api.setEnabled(a.account.id, e.currentTarget.checked).catch(fail)} /></td>
                  <td class="acts">
                    <button class="ic" title="Read this account now" aria-label="Read this account now" disabled={one === a.account.id} onClick={() => refresh(a.account.id)}>
                      <span class={one === a.account.id ? "spin" : ""}><I.Refresh /></span>
                    </button>
                    <button class="ic danger" title="Delete account" aria-label="Delete account" onClick={() => remove(a)}><I.Trash /></button>
                  </td>
                </tr>
                {s?.error && <tr class="errrow"><td colSpan={10}>{s.error}</td></tr>}
                {!s?.error && s?.margin_error && <tr class="errrow warnrow"><td colSpan={10}>Margin: {s.margin_error}</td></tr>}
              </>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function ClientPage({ id }: { id: string }) {
  const o = overview.value;
  const [adding, setAdding] = useState(false);
  if (!o) return <Loading />;
  const client = o.clients.find((c) => c.id === id);
  if (!client) {
    return (
      <section class="screen">
        <p class="empty-line">No such client. <a href="#/clients">Back to clients</a></p>
      </section>
    );
  }
  const accounts = o.accounts.filter((a) => a.account.client_id === id);
  const positions = o.positions.filter((p) => p.client_id === id);
  const t = totals(accounts, positions);
  return (
    <section class="screen">
      <div class="strip">
        <a class="back" href="#/clients" aria-label="Back to clients"><I.ChevUp /></a>
        <h1>{client.name}</h1>
        <span class="count">{client.client_code}{client.phone ? ` · ${client.phone}` : ""}{client.email ? ` · ${client.email}` : ""}</span>
        <span class="grow" />
        <button class="btn primary" disabled={adding} onClick={() => setAdding(true)}><I.Plus /> Add broker account</button>
      </div>
      <Figures
        items={[
          { label: "Accounts signed in", value: <>{t.signedIn}<span class="of"> of {t.accounts}</span></> },
          { label: "Available margin", value: <Money n={t.available} /> },
          { label: "Used margin", value: <Money n={t.used} /> },
          { label: "Open positions", value: t.open },
          { label: "Open P&L", value: <Money n={t.pnl} plus tone /> },
        ]}
      />
      {adding && <AddAccount client={client} onDone={() => setAdding(false)} />}
      <div class="block">
        <div class="strip"><h2>Broker accounts</h2><span class="count">{accounts.length}</span></div>
        {accounts.length === 0 ? (
          <div class="canvas"><p class="empty-line">No broker account yet. Press Add broker account.</p></div>
        ) : (
          <AccountsTable accounts={accounts} />
        )}
      </div>
      <PositionsTable rows={positions} showClient={false} empty="No open positions for this client." />
    </section>
  );
}

// ----------------------------------------------------------------- positions

function PositionsPage() {
  const o = overview.value;
  if (!o) return <Loading />;
  return (
    <section class="screen">
      <PositionsTable rows={o.positions} empty="No open positions yet. Press Read brokers to fetch them." />
    </section>
  );
}

// --------------------------------------------------------------------- app

export function App() {
  useEffect(() => {
    void api.me().then(() => {
      if (signedIn.value) api.loadOverview().catch(fail);
    });
  }, []);

  // While signed in: show stored data every 15 s, and read the brokers every 30 s, only when the page is in front.
  useEffect(() => {
    if (!signedIn.value) return;
    const show = window.setInterval(() => document.visibilityState === "visible" && api.loadOverview().catch(() => {}), 15000);
    const read = window.setInterval(() => document.visibilityState === "visible" && void readBrokers(), 30000);
    return () => {
      clearInterval(show);
      clearInterval(read);
    };
  }, [signedIn.value]);

  if (signedIn.value === null) return <div class="splash" role="status" aria-label="Loading"><span class="mark"><I.Mark /></span></div>;
  if (!signedIn.value) return <Login />;

  const r = route.value.split("?")[0];
  let page = <Overview />;
  if (r === "#/clients") page = <ClientsGrid />;
  else if (r.startsWith("#/clients/")) page = <ClientPage id={r.slice("#/clients/".length)} key={r} />;
  else if (r === "#/positions") page = <PositionsPage />;
  return <Shell>{page}</Shell>;
}
