// The positions table: grouped by broker account or by strategy, each leg split into what a trader
// reads (underlying, expiry, strike, call or put), with a strategy tag the admin can set. Legs are
// picked with the checkboxes, then added to a named strategy; a position that has no strategy yet is
// marked New, and one click sends it to a strategy.

import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { effect, signal } from "@preact/signals";
import { api, overview, type Leg, type Position, type Strategy } from "./api";
import { BROKERS, type BrokerId } from "./brokers";
import { Avatar, fail, Money, qty, say } from "./ui";
import { parseContract, productLabel, strikeText, moneyText } from "./format";
import * as I from "./icons";

// ------------------------------------------------------------------- state

const legKey = (p: Position) => `${p.account_id}|${p.trading_symbol}|${p.product}`;
const legOf = (p: Position): Leg => ({ account_id: p.account_id, trading_symbol: p.trading_symbol, product: p.product });

/** The legs ticked, by key. Kept across refreshes: a key whose position is gone is simply ignored. */
const picked = signal<Set<string>>(new Set());

// What the admin chose on this screen is kept between visits: how it is grouped, sorted and filtered,
// and which groups are folded. (Browser storage can be blocked; then the choices last this visit only.)
type SortKey = "contract" | "strike" | "qty" | "pnl";
type SortState = { key: SortKey; dir: 1 | -1 };
const PREFS = "rms.positions.v1";
const FILTERS = ["CE", "PE", "FUT", "WEEKLY", "LOOSE"] as const;
const saved: { view?: string; sort?: SortState; filters?: string[]; shut?: string[] } = (() => {
  try { return JSON.parse(localStorage.getItem(PREFS) ?? "{}"); } catch { return {}; }
})();
const view = signal<"account" | "strategy">(saved.view === "strategy" ? "strategy" : "account");
const sort = signal<SortState>(saved.sort && ["contract", "strike", "qty", "pnl"].includes(saved.sort.key) && (saved.sort.dir === 1 || saved.sort.dir === -1) ? saved.sort : { key: "contract", dir: 1 });
const filters = signal<Set<string>>(new Set((saved.filters ?? []).filter((f) => (FILTERS as readonly string[]).includes(f))));
const shut = signal<Set<string>>(new Set(Array.isArray(saved.shut) ? saved.shut.filter((x) => typeof x === "string") : []));
effect(() => {
  try { localStorage.setItem(PREFS, JSON.stringify({ view: view.value, sort: sort.value, filters: [...filters.value], shut: [...shut.value] })); } catch { /* storage blocked */ }
});
type Sheet = { kind: "add"; legs: Position[] } | { kind: "manage"; strategy: Strategy } | null;
const sheet = signal<Sheet>(null);

function setPicked(keys: string[], on: boolean) {
  const next = new Set(picked.value);
  for (const k of keys) (on ? next.add(k) : next.delete(k));
  picked.value = next;
}

// ------------------------------------------------------------- the contract

/** The contract split into what a trader reads: the underlying and its expiry, then the strike with call, put or future beside it. */
function ContractCells({ symbol, product, broker }: { symbol: string; product: string; broker: string }) {
  const c = parseContract(symbol, broker);
  const tag = productLabel(product);
  if (!c) return <><td class="name" title={symbol}>{symbol}<span class="prod" title={product}>{tag}</span></td><td class="strikecell sub">–</td></>;
  return (
    <>
      <td class="name" title={symbol}>
        <span class="und">{c.underlying}</span>
        <span class="exp">{c.expiry}</span>
        {c.weekly && <span class="wk" title="Weekly expiry">WEEKLY</span>}
        <span class="prod" title={product}>{tag}</span>
      </td>
      <td class="strikecell"><span class="snum">{c.strike === null ? "–" : strikeText(c.strike)}</span><span class={"kind " + c.kind.toLowerCase()}>{c.kind}</span></td>
    </>
  );
}

/** Underlying, then expiry date, then strike, calls before puts: the order a trader reads a book in. */
function byContract(a: Position, b: Position): number {
  const x = parseContract(a.trading_symbol, a.broker);
  const y = parseContract(b.trading_symbol, b.broker);
  if (!x || !y) return x ? -1 : y ? 1 : a.trading_symbol.localeCompare(b.trading_symbol);
  const rank = { CE: 0, PE: 1, FUT: 2 };
  return x.underlying.localeCompare(y.underlying) || x.sortKey - y.sortKey || (x.strike ?? -1) - (y.strike ?? -1) || rank[x.kind] - rank[y.kind];
}

function sortRows(rows: Position[]): Position[] {
  const { key, dir } = sort.value;
  const by: Record<SortKey, (a: Position, b: Position) => number> = {
    contract: byContract,
    strike: (a, b) => (parseContract(a.trading_symbol, a.broker)?.strike ?? -1) - (parseContract(b.trading_symbol, b.broker)?.strike ?? -1),
    qty: (a, b) => a.net_quantity - b.net_quantity,
    pnl: (a, b) => (a.pnl ?? 0) - (b.pnl ?? 0),
  };
  return [...rows].sort((a, b) => dir * (by[key](a, b) || byContract(a, b)));
}

/** A column heading that sorts: click once to sort, again to reverse. */
function SortTh({ k, label, cls = "" }: { k: SortKey; label: string; cls?: string }) {
  const on = sort.value.key === k;
  const dir = sort.value.dir;
  return (
    <th class={cls + " sortable"} aria-sort={on ? (dir === 1 ? "ascending" : "descending") : "none"}>
      <button class="sortbtn" onClick={() => (sort.value = on ? { key: k, dir: dir === 1 ? -1 : 1 } : { key: k, dir: k === "pnl" || k === "qty" ? -1 : 1 })} title={`Sort by ${label.toLowerCase()}`}>
        {label}
        <span class={"sortarrow" + (on ? " on" : "")}>{on ? (dir === 1 ? <I.ChevUp /> : <I.ChevDown />) : <I.ChevDown />}</span>
      </button>
    </th>
  );
}

// --------------------------------------------------------- the strategy tag

function StrategyCell({ p, byId }: { p: Position; byId: Map<string, Strategy> }) {
  const s = p.strategy_id ? byId.get(p.strategy_id) : undefined;
  return s ? (
    <button class="schip" onClick={() => (sheet.value = { kind: "add", legs: [p] })} title={`${s.name} (${s.tag}). Click to move or remove this leg.`}>
      <i class="stag">{s.tag}</i><span class="sname">{s.name}</span>
    </button>
  ) : (
    <button class="schip none" onClick={() => (sheet.value = { kind: "add", legs: [p] })} title="This position is not in a strategy yet. Click to add it to one.">
      <i class="snew">New</i><span class="sname">Add to strategy</span>
    </button>
  );
}

// ---------------------------------------------------------- the picker sheet

function Sheet() {
  const s = sheet.value;
  const first = useRef<HTMLInputElement>(null);
  const [choice, setChoice] = useState<string>("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const all = overview.value?.strategies ?? [];
  const legs = s?.kind === "add" ? s.legs : [];
  const clientIds = [...new Set(legs.map((l) => l.client_id))];
  const clientId = clientIds.length === 1 ? clientIds[0] : "";
  const options = all.filter((x) => x.client_id === clientId);
  const opening = s ? (s.kind === "add" ? legs.map(legKey).join(",") : s.strategy.id) : "";

  useEffect(() => {
    // Start on the strategy these legs are already in (if they share one), else on nothing chosen.
    const shared = new Set(legs.map((l) => l.strategy_id));
    setChoice(shared.size === 1 && legs[0]?.strategy_id ? legs[0].strategy_id : options.length === 0 ? "new" : "");
    setName(s?.kind === "manage" ? s.strategy.name : "");
    setConfirmDelete(false);
    queueMicrotask(() => first.current?.focus());
  }, [opening]);

  useEffect(() => {
    if (!s) return;
    const key = (e: KeyboardEvent) => e.key === "Escape" && (sheet.value = null);
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [!!s]);

  if (!s) return null;
  const close = () => (sheet.value = null);

  async function addLegs() {
    if (busy || !legs.length) return;
    setBusy(true);
    try {
      const body = choice === "new" ? { client_id: clientId, new_name: name, legs: legs.map(legOf) } : { strategy_id: choice, legs: legs.map(legOf) };
      const r = await api.tagLegs(body);
      setPicked(legs.map(legKey), false);
      say(`${r.tagged} leg${r.tagged === 1 ? "" : "s"} added to ${r.strategy.name}`);
      close();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }
  async function removeLegs() {
    setBusy(true);
    try {
      const n = await api.untagLegs(legs.filter((l) => l.strategy_id).map(legOf));
      setPicked(legs.map(legKey), false);
      say(`${n} leg${n === 1 ? "" : "s"} taken out of their strategy`);
      close();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }
  async function saveName() {
    if (s?.kind !== "manage" || busy) return;
    setBusy(true);
    try {
      await api.renameStrategy(s.strategy.id, name);
      say("Strategy renamed");
      close();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }
  async function remove() {
    if (s?.kind !== "manage") return;
    setBusy(true);
    try {
      await api.deleteStrategy(s.strategy.id);
      say(`${s.strategy.name} deleted. Its positions are not in a strategy now.`);
      close();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  const tagged = legs.filter((l) => l.strategy_id).length;
  const canAdd = !!clientId && (choice === "new" ? name.trim().length > 0 : choice !== "") && !busy;
  const clientName = legs[0]?.client_name ?? "";

  return (
    <div class="scrim" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="sheet-title">
        {s.kind === "add" ? (
          <>
            <header class="sheadr">
              <div>
                <h2 id="sheet-title">Add to strategy</h2>
                <p class="sub">{legs.length === 1 ? legs[0].trading_symbol : `${legs.length} positions`}{clientId ? ` · ${clientName}` : ""}</p>
              </div>
              <button class="ic" onClick={close} aria-label="Close"><I.X /></button>
            </header>
            {!clientId ? (
              <div class="sbody"><p class="banner" role="alert">These positions belong to different clients. A strategy belongs to one client, so pick one client's positions at a time.</p></div>
            ) : (
              <div class="sbody">
                {options.length > 0 && (
                  <fieldset class="slist">
                    <legend>{clientName}'s strategies</legend>
                    {options.map((o) => (
                      <label key={o.id} class={"sopt" + (choice === o.id ? " on" : "")}>
                        <input type="radio" name="strategy" checked={choice === o.id} onChange={() => setChoice(o.id)} />
                        <i class="stag">{o.tag}</i>
                        <span class="sname">{o.name}</span>
                        <span class="scount">{o.legs} leg{o.legs === 1 ? "" : "s"}</span>
                      </label>
                    ))}
                  </fieldset>
                )}
                <label class={"sopt newopt" + (choice === "new" ? " on" : "")}>
                  <input type="radio" name="strategy" checked={choice === "new"} onChange={() => setChoice("new")} />
                  <I.Plus />
                  <span class="sname">New strategy</span>
                </label>
                {choice === "new" && (
                  <label class="fld">
                    <span class="lab">Strategy name</span>
                    <input ref={first} type="text" value={name} maxLength={60} placeholder="Iron condor, Straddle, Hedge" autocomplete="off" onInput={(e) => setName(e.currentTarget.value)} onKeyDown={(e) => e.key === "Enter" && canAdd && void addLegs()} />
                  </label>
                )}
              </div>
            )}
            <footer class="sfoot">
              {tagged > 0 && <button class="btn quiet" disabled={busy} onClick={removeLegs}>Take out of strategy</button>}
              <span class="grow" />
              <button class="btn quiet" onClick={close}>Cancel</button>
              {clientId && <button class="btn primary" disabled={!canAdd} onClick={addLegs}>{choice === "new" ? "Create and add" : `Add ${legs.length} leg${legs.length === 1 ? "" : "s"}`}</button>}
            </footer>
          </>
        ) : (
          <>
            <header class="sheadr">
              <div>
                <h2 id="sheet-title">Strategy</h2>
                <p class="sub">{s.strategy.tag} · {s.strategy.legs} leg{s.strategy.legs === 1 ? "" : "s"}</p>
              </div>
              <button class="ic" onClick={close} aria-label="Close"><I.X /></button>
            </header>
            <div class="sbody">
              <label class="fld">
                <span class="lab">Name</span>
                <input ref={first} type="text" value={name} maxLength={60} autocomplete="off" onInput={(e) => setName(e.currentTarget.value)} onKeyDown={(e) => e.key === "Enter" && name.trim() && void saveName()} />
              </label>
              {confirmDelete && <p class="banner" role="alert">Delete {s.strategy.name}? Its {s.strategy.legs} position{s.strategy.legs === 1 ? "" : "s"} stay open but are no longer in a strategy.</p>}
            </div>
            <footer class="sfoot">
              {confirmDelete ? (
                <button class="btn danger" disabled={busy} onClick={remove}>Yes, delete</button>
              ) : (
                <button class="btn quiet" disabled={busy} onClick={() => setConfirmDelete(true)}><I.Trash /> Delete</button>
              )}
              <span class="grow" />
              <button class="btn quiet" onClick={close}>Cancel</button>
              <button class="btn primary" disabled={busy || !name.trim() || name.trim() === s.strategy.name} onClick={saveName}>Save name</button>
            </footer>
          </>
        )}
      </div>
    </div>
  );
}

// ----------------------------------------------------------------- the table

interface Group {
  id: string;
  rows: Position[];
  strategy?: Strategy;
}

function GroupBox({ rows }: { rows: Position[] }) {
  const keys = rows.map(legKey);
  const n = keys.filter((k) => picked.value.has(k)).length;
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => { if (ref.current) ref.current.indeterminate = n > 0 && n < keys.length; }, [n, keys.length]);
  return (
    <input ref={ref} type="checkbox" checked={n > 0 && n === keys.length} aria-label="Select every leg in this group" onChange={(e) => setPicked(keys, e.currentTarget.checked)} onClick={(e) => e.stopPropagation()} />
  );
}

export function PositionsTable({ rows, showClient = true, empty }: { rows: Position[]; showClient?: boolean; empty: string }) {
  const [q, setQ] = useState("");
  const searchBox = useRef<HTMLInputElement>(null);
  const strategies = overview.value?.strategies ?? [];
  const byId = useMemo(() => new Map(strategies.map((s) => [s.id, s])), [strategies]);
  const f = filters.value;

  // "/" jumps to the search box, as on most trading screens.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (e.key === "/" && !e.ctrlKey && !e.metaKey && !e.altKey && !(t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || t.isContentEditable))) {
        e.preventDefault();
        searchBox.current?.focus();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, []);

  const shown = useMemo(() => {
    const t = q.trim().toLowerCase();
    const kinds = ["CE", "PE", "FUT"].filter((k) => f.has(k));
    return rows.filter((p) => {
      const c = parseContract(p.trading_symbol, p.broker);
      if (kinds.length && !(c && kinds.includes(c.kind))) return false;
      if (f.has("WEEKLY") && !c?.weekly) return false;
      if (f.has("LOOSE") && p.strategy_id) return false;
      if (!t) return true;
      return [p.trading_symbol, p.client_name, p.account_label, p.broker, p.product, byId.get(p.strategy_id ?? "")?.name ?? ""].some((x) => x.toLowerCase().includes(t));
    });
  }, [rows, q, byId, f]);
  const pnl = shown.reduce((t, p) => t + (p.pnl ?? 0), 0);

  const groups = useMemo<Group[]>(() => {
    const m = new Map<string, Group>();
    for (const p of shown) {
      const by = view.value === "strategy" ? (p.strategy_id ? p.strategy_id : `none:${p.client_id}`) : p.account_id;
      const g = m.get(by) ?? { id: by, rows: [], strategy: p.strategy_id ? byId.get(p.strategy_id) : undefined };
      g.rows.push(p);
      m.set(by, g);
    }
    const list = [...m.values()].map((g) => ({ ...g, rows: sortRows(g.rows) }));
    // In the strategy view the positions that are not in a strategy come first: they are the ones to deal with.
    if (view.value === "strategy") list.sort((a, b) => Number(!!a.strategy) - Number(!!b.strategy));
    return list;
  }, [shown, byId, view.value, sort.value]);

  const chosen = shown.filter((p) => picked.value.has(legKey(p)));
  const loose = rows.filter((p) => !p.strategy_id && p.net_quantity !== 0);
  const toggle = (id: string) => { const n = new Set(shut.value); if (n.has(id)) n.delete(id); else n.add(id); shut.value = n; };
  const flip = (k: string) => { const n = new Set(filters.value); if (n.has(k)) n.delete(k); else n.add(k); filters.value = n; };
  const anyFilter = f.size > 0 || q.trim() !== "";

  return (
    <div class="block">
      <div class="strip">
        <h2>Positions</h2>
        <span class="count">{shown.length === rows.length ? `${rows.length}` : `${shown.length} of ${rows.length}`}</span>
        <span class="grow" />
        <button class="btn" disabled={shown.length === 0} onClick={() => exportCsv(shown, byId)} title="Download the positions shown as a CSV file"><I.Download /> Export</button>
        <div class="seg" role="group" aria-label="Group positions by">
          <button class={view.value === "account" ? "on" : ""} aria-pressed={view.value === "account"} onClick={() => (view.value = "account")}>By account</button>
          <button class={view.value === "strategy" ? "on" : ""} aria-pressed={view.value === "strategy"} onClick={() => (view.value = "strategy")}>By strategy</button>
        </div>
        <label class="search">
          <I.Search />
          <input ref={searchBox} type="search" placeholder="Search positions" aria-label="Search positions" aria-keyshortcuts="/" value={q} onInput={(e) => setQ(e.currentTarget.value)} />
          <kbd class="kbdhint" aria-hidden="true">/</kbd>
        </label>
      </div>

      {rows.length > 0 && (
        <div class="fchips" role="group" aria-label="Filter positions">
          {([["CE", "Calls"], ["PE", "Puts"], ["FUT", "Futures"], ["WEEKLY", "Weekly"], ["LOOSE", "Not in a strategy"]] as const).map(([k, label]) => (
            <button key={k} class={"fchip" + (f.has(k) ? " on" : "")} aria-pressed={f.has(k)} onClick={() => flip(k)}>{label}</button>
          ))}
          {anyFilter && <button class="fchip clear" onClick={() => { filters.value = new Set(); setQ(""); }}>Clear filters</button>}
        </div>
      )}

      {chosen.length > 0 ? (
        <div class="selbar" role="region" aria-label="Selected positions">
          <b>{chosen.length} selected</b>
          <button class="btn primary" onClick={() => (sheet.value = { kind: "add", legs: chosen })}><I.Plus /> Add to strategy</button>
          {chosen.some((p) => p.strategy_id) && (
            <button class="btn" onClick={() => api.untagLegs(chosen.filter((p) => p.strategy_id).map(legOf)).then((n) => { setPicked(chosen.map(legKey), false); say(`${n} leg${n === 1 ? "" : "s"} taken out of their strategy`); }).catch(fail)}>Take out of strategy</button>
          )}
          <span class="grow" />
          <button class="btn quiet" onClick={() => setPicked(chosen.map(legKey), false)}>Clear</button>
        </div>
      ) : loose.length > 0 && !f.has("LOOSE") ? (
        <div class="selbar loose" role="status">
          <i class="snew">New</i>
          <span>{loose.length} open position{loose.length === 1 ? " is" : "s are"} not in a strategy yet.</span>
          <span class="grow" />
          <button class="btn" onClick={() => setPicked(loose.filter((p) => shown.includes(p)).map(legKey), true)}>Select {loose.length === 1 ? "it" : "them"}</button>
        </div>
      ) : null}

      <div class="canvas">
        {rows.length === 0 ? (
          <p class="empty-line">{empty}</p>
        ) : shown.length === 0 ? (
          <p class="empty-line">No position matches{anyFilter ? " these filters" : ""}. <button class="linkbtn" onClick={() => { filters.value = new Set(); setQ(""); }}>Clear them</button></p>
        ) : (
          <table class="rtable ptable">
            <thead>
              <tr>
                <th class="c-sel"><span class="vh">Select</span></th>
                <SortTh k="contract" label="Contract" />
                <SortTh k="strike" label="Strike" cls="num c-strike" />
                <th class="c-strat">Strategy</th>
                <SortTh k="qty" label="Quantity" cls="num c-qty" />
                <th class="num c-px">Average</th>
                <th class="num c-px">Last</th>
                <SortTh k="pnl" label="P&L" cls="num c-pnl" />
              </tr>
            </thead>
            {groups.map((g) => {
              const isOpen = !shut.value.has(g.id);
              const sum = g.rows.reduce((t, p) => t + (p.pnl ?? 0), 0);
              const first = g.rows[0];
              const brokerName = BROKERS[first.broker as BrokerId]?.name ?? first.broker;
              return (
                <tbody key={g.id}>
                  <tr class="ghead" onClick={() => toggle(g.id)}>
                    <td class="c-sel"><GroupBox rows={g.rows} /></td>
                    <td colSpan={6}>
                      <div class="gline">
                        <button class="gbtn" aria-expanded={isOpen} aria-label={`${isOpen ? "Hide" : "Show"} ${view.value === "strategy" ? (g.strategy?.name ?? "positions not in a strategy") : `positions of ${first.client_name}, ${brokerName}`}`}>
                          <span class={"chev" + (isOpen ? " open" : "")}><I.ChevDown /></span>
                          {view.value === "strategy" ? (
                            <>
                              {g.strategy ? <><i class="stag">{g.strategy.tag}</i><span class="gname">{g.strategy.name}</span></> : <><i class="snew">New</i><span class="gname">Not in a strategy</span></>}
                              {showClient && <span class="gacct">{first.client_name}</span>}
                            </>
                          ) : (
                            <>
                              {showClient && <><Avatar name={first.client_name} /><span class="gname">{first.client_name}</span></>}
                              <span class="gacct">{brokerName}<i>{first.account_label.replace(/^\w+ /, "")}</i></span>
                            </>
                          )}
                          <span class="gcount">{g.rows.length} {view.value === "strategy" ? "leg" : "position"}{g.rows.length === 1 ? "" : "s"}</span>
                        </button>
                        {view.value === "strategy" && g.strategy && (
                          <button class="ic" aria-label={`Rename or delete ${g.strategy.name}`} title="Rename or delete" onClick={(e) => { e.stopPropagation(); sheet.value = { kind: "manage", strategy: g.strategy! }; }}><I.Pencil /></button>
                        )}
                        {!g.strategy && view.value === "strategy" && (
                          <button class="btn" onClick={(e) => { e.stopPropagation(); sheet.value = { kind: "add", legs: g.rows }; }}>Add all to a strategy</button>
                        )}
                      </div>
                    </td>
                    <td class="num"><Money n={sum} plus tone /></td>
                  </tr>
                  {isOpen && g.rows.map((p, i) => {
                    const k = legKey(p);
                    const on = picked.value.has(k);
                    return (
                      <tr class={"rrow" + (on ? " picked" : "")} key={`${k}-${i}`} style={{ "--i": Math.min(i, 14) } as never}>
                        <td class="c-sel"><input type="checkbox" checked={on} aria-label={`Select ${p.trading_symbol}`} onChange={(e) => setPicked([k], e.currentTarget.checked)} /></td>
                        <ContractCells symbol={p.trading_symbol} product={p.product} broker={p.broker} />
                        <td class="c-strat"><StrategyCell p={p} byId={byId} /></td>
                        <td class={"num qty " + (p.net_quantity < 0 ? "short" : "long")}>{qty(p.net_quantity)}</td>
                        <td class="num"><Money n={p.average_price} /></td>
                        <td class="num"><Money n={p.last_price} /></td>
                        <td class="num"><Money n={p.pnl} plus tone /></td>
                      </tr>
                    );
                  })}
                </tbody>
              );
            })}
            <tfoot>
              <tr>
                <td colSpan={7} class="sub">Total P&amp;L{shown.length !== rows.length ? " of the positions shown" : ""}</td>
                <td class="num"><Money n={pnl} plus tone /></td>
              </tr>
            </tfoot>
          </table>
        )}
      </div>
      <Sheet />
    </div>
  );
}

/** Downloads the positions shown as a CSV file, one row per leg, with the contract split into its parts. */
function exportCsv(rows: Position[], byId: Map<string, Strategy>) {
  const cell = (v: string | number | null | undefined) => {
    const t = v === null || v === undefined ? "" : String(v);
    // A leading = + - @ can be run as a formula by a spreadsheet; a quote in front keeps it text.
    const safe = /^[=+\-@]/.test(t) && Number.isNaN(Number(t)) ? `'${t}` : t;
    return /[",\n]/.test(safe) ? `"${safe.replace(/"/g, '""')}"` : safe;
  };
  const head = ["Client", "Broker", "Account", "Symbol", "Underlying", "Expiry", "Strike", "Type", "Product", "Strategy", "Quantity", "Average", "Last", "P&L"];
  const lines = [head.join(",")];
  for (const p of rows) {
    const c = parseContract(p.trading_symbol, p.broker);
    lines.push([
      p.client_name, BROKERS[p.broker as BrokerId]?.name ?? p.broker, p.account_label.replace(/^\w+ /, ""), p.trading_symbol,
      c?.underlying, c?.expiry, c?.strike, c?.kind, productLabel(p.product), byId.get(p.strategy_id ?? "")?.name,
      p.net_quantity, p.average_price, p.last_price, p.pnl === null ? "" : moneyText(p.pnl).replace(/,/g, ""),
    ].map(cell).join(","));
  }
  const blob = new Blob(["\ufeff" + lines.join("\r\n")], { type: "text/csv;charset=utf-8" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = `positions-${new Date().toISOString().slice(0, 10)}.csv`;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  say(`${rows.length} position${rows.length === 1 ? "" : "s"} exported`);
}
