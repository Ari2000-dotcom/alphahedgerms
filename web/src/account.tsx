// The admin's own account: changing the sign-in password. Opened from the Admin block at the foot
// of the side bar.

import { useEffect, useRef, useState } from "preact/hooks";
import { signal } from "@preact/signals";
import { api } from "./api";
import { fail, say } from "./ui";
import * as I from "./icons";

export const passwordOpen = signal(false);

const MIN = 10;

/** The 8-digit code from whatever was typed or pasted: a whole line from the code file works as well as the bare number. */
const codeFrom = (v: string) => /\d{8}/.exec(v)?.[0] ?? v.replace(/\D/g, "").slice(0, 8);

export function PasswordSheet() {
  const first = useRef<HTMLInputElement>(null);
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [again, setAgain] = useState("");
  const [show, setShow] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const open = passwordOpen.value;

  useEffect(() => {
    if (!open) return;
    setCurrent(""); setNext(""); setAgain(""); setShow(false); setErr("");
    queueMicrotask(() => first.current?.focus());
    const key = (e: KeyboardEvent) => e.key === "Escape" && (passwordOpen.value = false);
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [open]);

  if (!open) return null;
  const close = () => (passwordOpen.value = false);

  const short = next.length > 0 && next.length < MIN;
  const mismatch = again.length > 0 && again !== next;
  const same = next.length > 0 && next === current;
  const ready = current !== "" && next.length >= MIN && again === next && !same && next.trim() === next && !busy;

  async function submit(e: Event) {
    e.preventDefault();
    if (!ready) return;
    setBusy(true);
    setErr("");
    try {
      await api.changePassword(current, next);
      say("Password changed. Other signed-in sessions were ended.");
      close();
    } catch (x) {
      setErr(x instanceof Error ? x.message : String(x));
      fail(x);
    } finally {
      setBusy(false);
    }
  }

  const field = (label: string, value: string, set: (v: string) => void, opts: { auto: string; ref?: boolean; invalid?: boolean; hint?: string }) => (
    <label class="fld">
      <span class="lab">{label}</span>
      <span class="inputwrap" data-invalid={opts.invalid ? "true" : undefined}>
        <I.Lock />
        {show
          ? <input ref={opts.ref ? first : undefined} type="text" value={value} autocomplete="off" spellcheck={false} onInput={(e) => set(e.currentTarget.value)} aria-invalid={opts.invalid ? "true" : undefined} />
          : <input ref={opts.ref ? first : undefined} type="password" value={value} autocomplete={opts.auto} onInput={(e) => set(e.currentTarget.value)} aria-invalid={opts.invalid ? "true" : undefined} />}
      </span>
      {opts.hint && <small class={opts.invalid ? "bad" : ""}>{opts.hint}</small>}
    </label>
  );

  return (
    <div class="scrim" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="pw-title">
      <form class="sheetform" onSubmit={submit}>
        <header class="sheadr">
          <div>
            <h2 id="pw-title">Change password</h2>
            <p class="sub">Used to sign in to this admin console</p>
          </div>
          <button type="button" class="ic" onClick={close} aria-label="Close"><I.X /></button>
        </header>
        <div class="sbody">
          {field("Current password", current, setCurrent, { auto: "current-password", ref: true })}
          {field("New password", next, setNext, { auto: "new-password", invalid: short || same, hint: same ? "The new password must differ from the current one." : short ? `At least ${MIN} characters (${next.length} so far).` : `At least ${MIN} characters. A longer phrase is stronger.` })}
          {field("Confirm new password", again, setAgain, { auto: "new-password", invalid: mismatch, hint: mismatch ? "The two new passwords do not match." : undefined })}
          <label class="chk">
            <input type="checkbox" checked={show} onChange={(e) => setShow(e.currentTarget.checked)} />
            <span>Show passwords</span>
          </label>
          {err && <p class="banner" role="alert">{err}</p>}
          <p class="sub fine">Changing it signs out every other browser that is signed in. This one stays signed in.</p>
        </div>
        <footer class="sfoot">
          <span class="grow" />
          <button type="button" class="btn quiet" onClick={close}>Cancel</button>
          <button type="submit" class="btn primary" disabled={!ready}>{busy ? "Saving…" : "Change password"}</button>
        </footer>
      </form>
      </div>
    </div>
  );
}

/** The sign-in page's "forgot password": ask for a one-time code, find it on the server, then set a new password. */
export function ForgotPanel({ onBack }: { onBack: () => void }) {
  const [step, setStep] = useState<"ask" | "enter">("ask");
  const [info, setInfo] = useState<{ minutes: number; file: string } | null>(null);
  const [code, setCode] = useState("");
  const [next, setNext] = useState("");
  const [again, setAgain] = useState("");
  const [show, setShow] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  async function ask() {
    if (busy) return;
    setBusy(true);
    setErr("");
    try {
      setInfo(await api.forgot());
      setStep("enter");
    } catch (x) {
      setErr(x instanceof Error ? x.message : String(x));
    } finally {
      setBusy(false);
    }
  }
  const short = next.length > 0 && next.length < MIN;
  const mismatch = again.length > 0 && again !== next;
  const codeValue = codeFrom(code);
  const codeBad = code.trim() !== "" && !/^\d{8}$/.test(codeValue) && (/\D/.test(code.trim()) || code.trim().length > 8);
  const ready = /^\d{8}$/.test(codeValue) && next.length >= MIN && again === next && next.trim() === next && !busy;

  async function submit(e: Event) {
    e.preventDefault();
    if (!ready) return;
    setBusy(true);
    setErr("");
    try {
      await api.resetPassword(codeValue, next);
      say("Password changed. You are signed in.");
    } catch (x) {
      setErr(x instanceof Error ? x.message : String(x));
    } finally {
      setBusy(false);
    }
  }
  const pwInput = (value: string, set: (v: string) => void, auto: string) =>
    show
      ? <input type="text" value={value} autocomplete="off" spellcheck={false} onInput={(e) => set(e.currentTarget.value)} />
      : <input type="password" value={value} autocomplete={auto} onInput={(e) => set(e.currentTarget.value)} />;

  return (
    <div class="forgot">
      <div class="loginhead">
        <span class="mark big"><I.Mark /></span>
        <h1>Reset password</h1>
        <p class="sub">{step === "ask" ? "A one-time code proves you can reach the server." : "Enter the code and choose a new password."}</p>
      </div>
      {step === "ask" ? (
        <>
          <p class="sub fine">The code is not shown here. It is saved in a file on the computer that runs the RMS server, and printed in the server's window. Press the button, then open that file.</p>
          {err && <p class="banner" role="alert">{err}</p>}
          <button class="btn primary lg" type="button" disabled={busy} onClick={ask}>{busy ? "Making a code…" : "Make a reset code"}</button>
          <button class="btn quiet" type="button" onClick={onBack}>Back to sign in</button>
        </>
      ) : (
        <form class="forgotform" onSubmit={submit}>
          <p class="note">Open <b>{info?.file}</b> on the server computer. The code works for {info?.minutes} minutes and five tries.</p>
          <label class="fld">
            <span class="lab">Reset code</span>
            <span class="inputwrap"><I.Key /><input type="text" inputMode="numeric" value={code} autofocus autocomplete="one-time-code" placeholder="8 digits" onInput={(e) => setCode(e.currentTarget.value)} aria-invalid={codeBad ? "true" : undefined} /></span>
            {codeBad && <small class="bad">That is not a reset code. The code is 8 digits, from the file on the server. To use your current password, go back to sign in.</small>}
          </label>
          <label class="fld">
            <span class="lab">New password</span>
            <span class="inputwrap" data-invalid={short ? "true" : undefined}><I.Lock />{pwInput(next, setNext, "new-password")}</span>
            <small class={short ? "bad" : ""}>{short ? `At least ${MIN} characters (${next.length} so far).` : `At least ${MIN} characters.`}</small>
          </label>
          <label class="fld">
            <span class="lab">Confirm new password</span>
            <span class="inputwrap" data-invalid={mismatch ? "true" : undefined}><I.Lock />{pwInput(again, setAgain, "new-password")}</span>
            {mismatch && <small class="bad">The two passwords do not match.</small>}
          </label>
          <label class="chk"><input type="checkbox" checked={show} onChange={(e) => setShow(e.currentTarget.checked)} /><span>Show passwords</span></label>
          {err && <p class="banner" role="alert">{err}</p>}
          <button class="btn primary lg" type="submit" disabled={!ready}>{busy ? "Saving…" : "Set new password"}</button>
          <button class="btn quiet" type="button" onClick={onBack}>Back to sign in</button>
        </form>
      )}
    </div>
  );
}
