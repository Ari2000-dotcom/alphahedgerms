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
