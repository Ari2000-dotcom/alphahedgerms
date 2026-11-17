// The Clients grid, built the way the copy-trading desk's accounts grid is: one row per broker
// account, the sign-in details as columns you type into, a new row at the top for adding, and
// hints for the broker you pick. Enter saves a row and signs it in; Escape throws the edit away.

import { useMemo, useState } from "preact/hooks";
import { signal } from "@preact/signals";
import { api, overview, type AccountView } from "./api";
import { BROKERS, BROKER_ORDER, CRED_COLS, fieldFor, type BrokerId, type Field } from "./brokers";
import { Avatar, fail, go, Money, say } from "./ui";
import * as I from "./icons";

// ------------------------------------------------------------------- edits

/** Something typed into a row and not saved yet. */
interface Draft {
  client_name?: string;
  broker?: BrokerId;
  /** Only the cells that were touched, by field key. */
  vals: Record<string, string>;
  /** Cells flagged when a save was refused. */
  invalid: string[];
}

const NEW = "new";
const drafts = signal<Record<string, Draft>>({});
const adding = signal(false);
const saving = signal<Set<string>>(new Set());

const draftOf = (id: string): Draft => drafts.value[id] ?? { vals: {}, invalid: [] };

function patch(id: string, p: Partial<Omit<Draft, "vals">> & { val?: [string, string] }) {
  const old = draftOf(id);
  const { val, ...rest } = p;
  drafts.value = {
    ...drafts.value,
    [id]: { ...old, ...rest, vals: val ? { ...old.vals, [val[0]]: val[1] } : old.vals, invalid: val ? old.invalid.filter((k) => k !== val[0]) : old.invalid },
  };
}

function drop(id: string) {
  const { [id]: _gone, ...rest } = drafts.value;
  drafts.value = rest;
  if (id === NEW) adding.value = false;
}

function flag(id: string, keys: string[], msg: string) {
  drafts.value = { ...drafts.value, [id]: { ...draftOf(id), invalid: keys } };
  say(msg);
}

const setSaving = (id: string, on: boolean) => {
  const next = new Set(saving.value);
  if (on) next.add(id); else next.delete(id);
  saving.value = next;
};

/** What the server has for a plain field. The user ID is the account's own id. */
function current(a: AccountView, f: Field): string {
  return f.key === "userId" ? a.account.broker_user_id : a.fields[f.key] ?? "";
}

function dirty(a: AccountView): boolean {
  const d = drafts.value[a.account.id];
  if (!d) return false;
  const broker = a.account.broker as BrokerId;
  return Object.entries(d.vals).some(([k, v]) => {
    const f = BROKERS[broker].fields.find((x) => x.key === k);
    return f?.secret ? v.trim() !== "" : v.trim() !== "" && v.trim() !== (f ? current(a, f) : "");
  });
}

/** Splits typed values into what the server expects: the id, the secrets, the plain fields. */
function payload(broker: BrokerId, vals: Record<string, string>) {
  let user_id = "";
  const secrets: Record<string, string> = {};
  const fields: Record<string, string> = {};
  for (const f of BROKERS[broker].fields) {
    const v = (vals[f.key] ?? "").trim();
    if (!v) continue;
    if (f.key === "userId") user_id = v;
    else if (f.secret) secrets[f.key] = v;
    else fields[f.key] = v;
  }
  return { user_id, secrets, fields };
}

async function saveNew() {
  if (saving.value.has(NEW)) return;
  const d = draftOf(NEW);
  const broker = d.broker ?? "zerodha";
  const def = BROKERS[broker];
  const name = (d.client_name ?? "").trim();
  if (!name) return flag(NEW, ["client"], "Enter the client's name");
  const missing = def.fields.filter((f) => !(d.vals[f.key] ?? "").trim());
  if (missing.length) return flag(NEW, missing.map((f) => f.key), `${def.name} needs: ${missing.map((f) => f.label).join(", ")}`);
  setSaving(NEW, true);
  try {
    const r = await api.createAccount({ client_name: name, broker, ...payload(broker, d.vals) });
    drop(NEW);
    say(r.status.signed_in ? `${name}: ${def.name} account added and signed in` : `${name}: saved, but sign-in failed. ${r.status.error ?? ""}`);
  } catch (e) {
    fail(e);
  } finally {
    setSaving(NEW, false);
  }
}

async function saveExisting(a: AccountView) {
  const id = a.account.id;
  if (saving.value.has(id) || !dirty(a)) return;
  const broker = a.account.broker as BrokerId;
  setSaving(id, true);
  try {
    const r = await api.updateAccount(id, payload(broker, draftOf(id).vals));
    drop(id);
    say(r.status.signed_in ? `${a.account.client_name}: updated and signed in` : `${a.account.client_name}: updated, but sign-in failed. ${r.status.error ?? ""}`);
  } catch (e) {
    fail(e);
  } finally {
    setSaving(id, false);
  }
}

const save = (a: AccountView | null) => (a ? saveExisting(a) : saveNew());

/** A blank row for the draft being added. It is not an account yet. */
function blank(): AccountView {
  const d = draftOf(NEW);
  return {
    account: { id: NEW, client_id: "", client_code: "", client_name: d.client_name ?? "", broker: d.broker ?? "zerodha", broker_user_id: "", label: "", enabled: true, created_at: "" },
    status: null, margin: null, saved: {}, fields: {},
  };
}

/** Opens the add row (used from the overview's empty state). */
export function startAdding() {
  adding.value = true;
}

// ------------------------------------------------------------------ the page

const rowKeys = (a: AccountView | null) => (e: KeyboardEvent) => {
  if (e.key === "Enter") { e.preventDefault(); void save(a); }
  if (e.key === "Escape") { e.preventDefault(); drop(a ? a.account.id : NEW); }
};

function SignInCell({ a, col }: { a: AccountView; col: (typeof CRED_COLS)[number]["id"] }) {
  const isNew = a.account.id === NEW;
  const broker = (isNew ? draftOf(NEW).broker ?? "zerodha" : a.account.broker) as BrokerId;
  const f = fieldFor(broker, col);
  if (!f) return <td class="na">–</td>;
  const d = draftOf(a.account.id);
  const savedHere = f.secret && !!a.saved[f.key];
  const value = d.vals[f.key] ?? (f.secret || isNew ? "" : current(a, f));
  const common = {
    class: "cellin" + (savedHere && !value ? " saved" : ""),
    name: `${a.account.id}-${f.key}`,
    value,
    placeholder: savedHere ? "••••••••" : f.label,
    title: savedHere && !value ? `${f.label} is saved. Type to replace it.` : `${f.label}. ${f.where}`,
    "aria-label": `${a.account.client_name || "New client"}: ${f.label}${savedHere ? " (saved)" : ""}`,
    "aria-invalid": d.invalid.includes(f.key) ? ("true" as const) : undefined,
    autocomplete: "off",
    spellcheck: false,
    onInput: (e: { currentTarget: HTMLInputElement }) => { const v = e.currentTarget.value; patch(a.account.id, { val: [f.key, v] }); },
    onKeyDown: rowKeys(isNew ? null : a),
  };
  return <td class="cellc">{f.secret ? <input type="password" {...common} /> : <input type="text" {...common} />}</td>;
}

function Status({ a }: { a: AccountView }) {
  if (a.account.id === NEW) return <span class="state"><i class="dot mid" />New</span>;
  const s = a.status;
  if (!s) return <span class="state"><i class="dot" />Not read yet</span>;
  if (s.signed_in) return <span class="state ok"><i class="dot ok" />Signed in</span>;
  return <span class="state linklike"><i class="dot bad" />Sign-in failed</span>;
}

function Row({ a, i, ps, names }: { a: AccountView; i: number; ps: { open: number; pnl: number }; names: string[] }) {
  const isNew = a.account.id === NEW;
  const id = a.account.id;
  const d = draftOf(id);
  const m = a.margin;
  const busy = saving.value.has(id);
  const isDirty = isNew || dirty(a);
  const [one, setOne] = useState(false);
  const err = !isNew && a.status && a.status.error ? a.status.error : "";
  const warn = !isNew && !err && a.status?.margin_error ? a.status.margin_error : "";
  const nameInput = (
    <input
      class="cellin name" name="client-name" list="client-names" value={d.client_name ?? ""} placeholder="Client name" autocomplete="off" autofocus
      aria-label="Client name" aria-invalid={d.invalid.includes("client") ? "true" : undefined}
      onInput={(e) => patch(NEW, { client_name: e.currentTarget.value })} onKeyDown={rowKeys(null)}
    />
  );
  return (
    <>
      <tr class={"rrow" + (isNew ? " new" : "") + (isDirty && !isNew ? " dirty" : "")} style={{ "--i": Math.min(i, 14) } as never}>
        <td class={isNew ? "cellc" : "name"}>
          {isNew ? nameInput : <a class="who" href={`#/clients/${a.account.client_id}`}><Avatar name={a.account.client_name} />{a.account.client_name}</a>}
        </td>
        <td class="sub">{isNew ? (names.includes((d.client_name ?? "").trim().toLowerCase()) ? "existing" : "auto") : a.account.client_code}</td>
        <td class={isNew ? "cellc" : ""}>
          {isNew ? (
            <select class="cellin" aria-label="Broker" value={d.broker ?? "zerodha"} onChange={(e) => patch(NEW, { broker: e.currentTarget.value as BrokerId, invalid: [] })}>
              {BROKER_ORDER.map((b) => <option key={b} value={b}>{BROKERS[b].name}</option>)}
            </select>
          ) : (
            BROKERS[a.account.broker as BrokerId]?.name ?? a.account.broker
          )}
        </td>
        <td><Status a={a} /></td>
        <td class="num">{isNew ? "–" : ps.open}</td>
        <td class="num">{isNew ? "–" : ps.open || ps.pnl ? <Money n={ps.pnl} plus tone /> : "–"}</td>
        <td class="num"><Money n={m?.available} /></td>
        <td class="num"><Money n={m?.used} /></td>
        <td class="num"><Money n={m?.total} /></td>
        <td class="num">{m && m.cash === null ? <span class="money zero" title="This broker does not report cash on its own">–</span> : <Money n={m?.cash} />}</td>
        <td class="num"><Money n={m?.collateral} /></td>
        {CRED_COLS.map((c) => <SignInCell key={c.id} a={a} col={c.id} />)}
        <td class="acts">
          {isDirty ? (
            <>
              <button class="ic" title="Save and sign in (Enter)" aria-label="Save" disabled={busy} onClick={() => save(isNew ? null : a)}>
                <span class={busy ? "spin" : ""}>{busy ? <I.Refresh /> : <I.Check />}</span>
              </button>
              <button class="ic" title="Discard (Esc)" aria-label="Discard" disabled={busy} onClick={() => drop(id)}><I.X /></button>
            </>
          ) : (
            <>
              <button class="ic" title="Read this account now" aria-label="Read this account now" disabled={one} onClick={async () => { setOne(true); try { await api.refreshOne(id); } catch (e) { fail(e); } finally { setOne(false); } }}>
                <span class={one ? "spin" : ""}><I.Refresh /></span>
              </button>
              <label class="activebox" title="Active: included in reads and totals"><input type="checkbox" checked={a.account.enabled} aria-label={`${a.account.label} active`} onChange={(e) => api.setEnabled(id, e.currentTarget.checked).catch(fail)} /></label>
              <button class="ic danger" title="Delete account" aria-label="Delete account" onClick={() => { if (confirm(`Delete ${a.account.client_name}'s ${a.account.label}? Its saved sign-in details are removed. Positions and orders at the broker are not touched.`)) api.deleteAccount(id).then(() => say("Account deleted")).catch(fail); }}>
                <I.Trash />
              </button>
            </>
          )}
        </td>
      </tr>
      {err && <tr class="errrow"><td colSpan={19}>{err}</td></tr>}
      {warn && <tr class="errrow warnrow"><td colSpan={19}>Margin: {warn}</td></tr>}
    </>
  );
}

/** While a row is being filled in: what the broker needs, and where to find the next thing. */
function HintBar({ a, names }: { a: AccountView; names: Map<string, string> }) {
  const isNew = a.account.id === NEW;
  const d = draftOf(a.account.id);
  const broker = (isNew ? d.broker ?? "zerodha" : a.account.broker) as BrokerId;
  const def = BROKERS[broker];
  const done = (f: Field) => {
    const typed = (d.vals[f.key] ?? "").trim();
    if (typed) return true;
    return f.secret ? !!a.saved[f.key] : !isNew && !!current(a, f);
  };
  const next = def.fields.find((f) => !done(f));
  const existing = isNew ? names.get((d.client_name ?? "").trim().toLowerCase()) : undefined;
  return (
    <div class="hintbar" role="status">
      <b>{isNew ? `${def.name} needs` : `Editing ${a.account.client_name}: ${def.name}`}</b>
      {def.fields.map((f) => (
        <span key={f.key} class={"need" + (done(f) ? " done" : "")} title={f.where}>{done(f) && <I.Check />}{f.label}</span>
      ))}
      <span class="next">
        {next ? <><strong>{next.label}.</strong> {next.where}</> : isNew ? "All filled in. Press Enter, or the tick at the end of the row, to add the client and sign in." : "Press Enter, or the tick at the end of the row, to save and sign in again."}
      </span>
      {existing && <span class="next">This name is already a client ({existing}). The new account is added to them.</span>}
      {!existing && isNew && next && def.note && <span class="next">{def.note}</span>}
    </div>
  );
}

export function ClientsGrid() {
  const o = overview.value;
  const [q, setQ] = useState("");
  const names = useMemo(() => new Map((o?.clients ?? []).map((c) => [c.name.trim().toLowerCase(), c.name])), [o]);
  if (!o) return null;
  const t = q.trim().toLowerCase();
  const rows = o.accounts.filter((a) => !t || [a.account.client_name, a.account.client_code, a.account.broker, a.account.broker_user_id, a.account.label].some((x) => x.toLowerCase().includes(t)));
  const byAccount = new Map<string, { open: number; pnl: number }>();
  for (const p of o.positions) {
    const x = byAccount.get(p.account_id) ?? { open: 0, pnl: 0 };
    byAccount.set(p.account_id, { open: x.open + (p.net_quantity !== 0 ? 1 : 0), pnl: x.pnl + (p.pnl ?? 0) });
  }
  const none = { open: 0, pnl: 0 };
  const editing = adding.value ? blank() : o.accounts.find((a) => dirty(a)) ?? null;

  return (
    <section class="screen wide">
      <div class="strip">
        <h1>Clients</h1>
        <span class="count">{o.clients.length} client{o.clients.length === 1 ? "" : "s"} · {o.accounts.length} broker account{o.accounts.length === 1 ? "" : "s"}</span>
        <span class="rule" />
        <label class="search">
          <I.Search />
          <input type="search" placeholder="Search clients" aria-label="Search clients" value={q} onInput={(e) => setQ(e.currentTarget.value)} />
        </label>
        <span class="grow" />
        <button class="btn primary" disabled={adding.value} onClick={() => { adding.value = true; }}><I.Plus /> Add client</button>
      </div>

      {editing && <HintBar a={editing} names={names} />}
      <datalist id="client-names">{o.clients.map((c) => <option key={c.id} value={c.name} />)}</datalist>

      {rows.length === 0 && !adding.value ? (
        <div class="canvas"><div class="empty">
          <span class="glyph"><I.NavAccounts /></span>
          <h2>{o.accounts.length === 0 ? "Add your first client" : "No client matches"}</h2>
          <p>{o.accounts.length === 0 ? "Press Add client. A row appears where you type the client's name and their broker's sign-in details. Enter saves it and signs in, the same way the copy-trading desk adds an account." : "Try another search."}</p>
          {o.accounts.length === 0 && <button class="btn primary" onClick={() => { adding.value = true; }}><I.Plus /> Add client</button>}
        </div></div>
      ) : (
        <div class="canvas gridwrap">
          <table class="rtable gtable">
            <thead>
              <tr class="grp">
                <th colSpan={4}>Client</th>
                <th colSpan={2}>Positions</th>
                <th colSpan={5}>Margin</th>
                <th colSpan={7}>Sign-in details</th>
                <th />
              </tr>
              <tr>
                <th>Name</th><th>Code</th><th>Broker</th><th>Status</th>
                <th class="num">Open</th><th class="num">P&amp;L</th>
                <th class="num">Available</th><th class="num">Used</th><th class="num">Total</th><th class="num">Cash</th><th class="num">Collateral</th>
                {CRED_COLS.map((c) => <th key={c.id} style={{ minWidth: `${c.w}px` }}>{c.label}</th>)}
                <th><span class="vh">Actions</span></th>
              </tr>
            </thead>
            <tbody>
              {adding.value && <Row key={NEW} a={blank()} i={0} ps={none} names={[...names.keys()]} />}
              {rows.map((a, i) => <Row key={a.account.id} a={a} i={i} ps={byAccount.get(a.account.id) ?? none} names={[]} />)}
            </tbody>
          </table>
        </div>
      )}
      <p class="foot sub">Secrets are saved encrypted and never shown. Type over a saved cell to replace it. <button class="linkbtn" onClick={() => go("#/positions")}>See all positions</button></p>
    </section>
  );
}
