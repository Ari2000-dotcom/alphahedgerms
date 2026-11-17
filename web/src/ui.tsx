import { signal } from "@preact/signals";
import { moneyParts } from "./format";

// The message at the foot of the screen.
export const toast = signal("");
let toastTimer: number | undefined;
export function say(msg: string) {
  toast.value = msg;
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (toast.value = ""), 3600);
}
export const fail = (e: unknown) => say(e instanceof Error ? e.message : String(e));

export const go = (to: string) => (location.hash = to);

/** Money the way a broker screen shows it: Indian grouping, two decimals, dimmed decimals. */
export function Money({ n, plus = false, tone = false }: { n: number | null | undefined; plus?: boolean; tone?: boolean }) {
  if (n === null || n === undefined) return <span class="money zero">–</span>;
  const p = moneyParts(n, plus);
  if (!p) return <span class="money zero">–</span>;
  return (
    <span class={"money" + (tone ? ` pnl ${p.tone}` : "")}>
      {p.sign}{p.int}<span class="frac">{p.frac}</span>
    </span>
  );
}

export const qty = (n: number) => (n === 0 ? "0" : n.toLocaleString("en-IN"));
