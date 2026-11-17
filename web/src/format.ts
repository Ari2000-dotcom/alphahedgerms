// How money is shown, the way a broker screen does: Indian digit grouping
// (12,34,567), always two decimals, minus sign for losses, optional plus sign
// for profits. Pure functions, tested with `node --test src/format.test.ts`.

export interface MoneyParts {
  /** "-", "+" or "" */
  sign: string;
  /** Whole part with Indian grouping, e.g. "54,48,697" */
  int: string;
  /** Decimal part including the dot, e.g. ".00" */
  frac: string;
  /** Direction for colouring: a rounded-to-zero value is neutral. */
  tone: "pos" | "neg" | "zero";
}

const group = new Intl.NumberFormat("en-IN", { maximumFractionDigits: 0, useGrouping: true });

export function moneyParts(n: number, showPlus = false): MoneyParts | null {
  if (!Number.isFinite(n)) return null;
  // Round first, so -0.004 is shown as 0.00 and not as "-0.00".
  const cents = Math.round(Math.abs(n) * 100);
  const whole = Math.floor(cents / 100);
  const frac = "." + String(cents % 100).padStart(2, "0");
  const tone = cents === 0 ? "zero" : n < 0 ? "neg" : "pos";
  const sign = tone === "neg" ? "-" : tone === "pos" && showPlus ? "+" : "";
  return { sign, int: group.format(whole), frac, tone };
}

/** Plain text, for places that cannot take markup (titles, tooltips). */
export function moneyText(n: number, showPlus = false): string {
  const p = moneyParts(n, showPlus);
  return p ? `${p.sign}${p.int}${p.frac}` : "–";
}

// ------------------------------------------------------------------ contracts

export interface Contract {
  /** "NIFTY", "BANKNIFTY" */
  underlying: string;
  /** "Dec 2026" for a monthly expiry, "17 Nov 2026" for a weekly one */
  expiry: string;
  /** Strike price, or null for a future */
  strike: number | null;
  kind: "CE" | "PE" | "FUT";
  weekly: boolean;
  /** YYYYMMDD, for ordering by expiry. A monthly expiry has no day in Kite's symbol, so it sorts after that month's weeklies (day 99). */
  sortKey: number;
}

const key = (year: number, month: number, day: number) => year * 10000 + (month + 1) * 100 + day;
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKLY_MONTH: Record<string, number> = { "1": 0, "2": 1, "3": 2, "4": 3, "5": 4, "6": 5, "7": 6, "8": 7, "9": 8, O: 9, N: 10, D: 11 };

/**
 * Reads a derivatives symbol the way Kite writes it: NIFTY26DEC22600CE (monthly option),
 * NIFTY2611324500CE and NIFTY26N1725600CE (weekly: year, month as 1-9 or O N D, day, strike),
 * BANKNIFTY26NOVFUT (future). Anything else returns null and the symbol is shown as it came.
 */
export function parseContract(symbol: string, broker = ""): Contract | null {
  const s = symbol.trim().toUpperCase();
  if (broker === "angel") return parseAngel(s);
  let m = /^([A-Z&-]+?)(\d{2})([A-Z]{3})FUT$/.exec(s);
  if (m) {
    const mo = MONTHS.findIndex((x) => x.toUpperCase() === m![3]);
    if (mo >= 0) return { underlying: m[1], expiry: `${MONTHS[mo]} 20${m[2]}`, strike: null, kind: "FUT", weekly: false, sortKey: key(2000 + Number(m[2]), mo, 99) };
  }
  m = /^([A-Z&-]+?)(\d{2})([A-Z]{3})(\d+(?:\.\d+)?)(CE|PE)$/.exec(s);
  if (m) {
    const mo = MONTHS.findIndex((x) => x.toUpperCase() === m![3]);
    if (mo >= 0) return { underlying: m[1], expiry: `${MONTHS[mo]} 20${m[2]}`, strike: Number(m[4]), kind: m[5] as "CE" | "PE", weekly: false, sortKey: key(2000 + Number(m[2]), mo, 99) };
  }
  m = /^([A-Z&-]+?)(\d{2})([1-9OND])(\d{2})(\d+(?:\.\d+)?)(CE|PE)$/.exec(s);
  if (m) {
    const mo = WEEKLY_MONTH[m[3]];
    if (mo !== undefined) return { underlying: m[1], expiry: `${Number(m[4])} ${MONTHS[mo]} 20${m[2]}`, strike: Number(m[5]), kind: m[6] as "CE" | "PE", weekly: true, sortKey: key(2000 + Number(m[2]), mo, Number(m[4])) };
  }
  return null;
}

/** A strike with Indian grouping and no decimals unless it has some. */
export const strikeText = (n: number) => n.toLocaleString("en-IN", { maximumFractionDigits: 2 });

/** Angel One writes the full date first: NIFTY17NOV2526250PE is 17 Nov 2025, strike 26250. */
function parseAngel(s: string): Contract | null {
  let m = /^([A-Z&-]+?)(\d{2})([A-Z]{3})(\d{2})(\d+(?:\.\d+)?)(CE|PE)$/.exec(s);
  let fut = false;
  if (!m) {
    m = /^([A-Z&-]+?)(\d{2})([A-Z]{3})(\d{2})()FUT$/.exec(s);
    fut = !!m;
  }
  if (!m) return null;
  const mo = MONTHS.findIndex((x) => x.toUpperCase() === m![3]);
  if (mo < 0) return null;
  const day = Number(m[2]);
  const year = 2000 + Number(m[4]);
  // A monthly expiry is the last Tuesday or Thursday of its month; any other expiry date is a weekly.
  const d = new Date(Date.UTC(year, mo, day));
  const wd = d.getUTCDay();
  const lastOfWeekday = new Date(Date.UTC(year, mo, day + 7)).getUTCMonth() !== mo;
  const monthly = (wd === 2 || wd === 4) && lastOfWeekday;
  if (fut) return { underlying: m[1], expiry: `${MONTHS[mo]} ${year}`, strike: null, kind: "FUT", weekly: false, sortKey: key(year, mo, day) };
  return { underlying: m[1], expiry: monthly ? `${MONTHS[mo]} ${year}` : `${day} ${MONTHS[mo]} ${year}`, strike: Number(m[5]), kind: m[6] as "CE" | "PE", weekly: !monthly, sortKey: key(year, mo, day) };
}

/** Brokers name the same product differently; show the short names traders know. */
const PRODUCTS: Record<string, string> = { CARRYFORWARD: "NRML", INTRADAY: "MIS", DELIVERY: "CNC", MARGIN: "MARGIN", NORMAL: "NRML", NRML: "NRML", MIS: "MIS", CNC: "CNC" };
export const productLabel = (p: string) => PRODUCTS[p.trim().toUpperCase()] ?? p;
