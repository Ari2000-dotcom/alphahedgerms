// The admin API. The session is an HttpOnly cookie, so the page never holds a token.

import { signal } from "@preact/signals";

export interface Client {
  id: string;
  client_code: string;
  name: string;
  email: string | null;
  phone: string | null;
  notes: string | null;
  status: string;
  created_at: string;
  accounts: number;
}

export interface Account {
  id: string;
  client_id: string;
  client_code: string;
  client_name: string;
  broker: string;
  broker_user_id: string;
  label: string;
  enabled: boolean;
  created_at: string;
}

export interface Status {
  signed_in: boolean;
  error: string | null;
  margin_error: string | null;
  at: string;
}

export interface Margin {
  available: number;
  used: number;
  total: number;
  cash: number | null;
  collateral: number | null;
  taken_at: string;
}

export interface AccountView {
  account: Account;
  status: Status | null;
  margin: Margin | null;
  /** Which secrets are saved. Never their values. */
  saved: Record<string, boolean>;
  /** The plain fields (mobile, redirect URL). */
  fields: Record<string, string>;
}

export interface Position {
  account_id: string;
  client_id: string;
  client_name: string;
  broker: string;
  account_label: string;
  taken_at: string;
  trading_symbol: string;
  segment: string | null;
  token: string | null;
  product: string;
  net_quantity: number;
  average_price: number | null;
  last_price: number | null;
  pnl: number | null;
}

export interface Overview {
  clients: Client[];
  accounts: AccountView[];
  positions: Position[];
}

/** null until the first answer: the page shows nothing rather than a flash of the sign-in form. */
export const signedIn = signal<boolean | null>(null);
export const overview = signal<Overview | null>(null);
export const lastError = signal("");

export class ApiError extends Error {
  constructor(message: string, readonly status: number) {
    super(message);
  }
}

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  let res: Response;
  try {
    res = await fetch(`/api${path}`, {
      method,
      credentials: "same-origin",
      headers: body === undefined ? undefined : { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new ApiError("The RMS server is not reachable. Is it running?", 0);
  }
  const json = (await res.json().catch(() => ({}))) as { error?: string };
  if (res.status === 401 && path !== "/login") signedIn.value = false;
  if (!res.ok) throw new ApiError(json.error ?? `HTTP ${res.status}`, res.status);
  return json as T;
}

export const api = {
  async me() {
    try {
      signedIn.value = (await call<{ signed_in: boolean }>("GET", "/me")).signed_in;
    } catch {
      signedIn.value = false;
    }
  },
  async login(password: string) {
    await call("POST", "/login", { password });
    signedIn.value = true;
  },
  async logout() {
    await call("POST", "/logout");
    signedIn.value = false;
    overview.value = null;
  },
  async loadOverview() {
    overview.value = await call<Overview>("GET", "/overview");
    lastError.value = "";
  },
  /** Reads every enabled account from its broker, then reloads. */
  async refreshAll() {
    await call("POST", "/refresh");
    await api.loadOverview();
  },
  async refreshOne(id: string) {
    await call("POST", `/accounts/${id}/refresh`);
    await api.loadOverview();
  },
  async addClient(c: { client_code: string; name: string; email?: string; phone?: string; notes?: string }) {
    const r = await call<{ id: string }>("POST", "/clients", c);
    await api.loadOverview();
    return r.id;
  },
  async addAccount(clientId: string, a: { broker: string; user_id: string; label?: string; secrets: Record<string, string>; fields: Record<string, string> }) {
    const r = await call<{ id: string; status: Status }>("POST", `/clients/${clientId}/accounts`, a);
    await api.loadOverview();
    return r;
  },
  /** Adds a client and their broker account in one step. The same client name finds the same client. */
  async createAccount(a: { client_name: string; broker: string; user_id: string; secrets: Record<string, string>; fields: Record<string, string> }) {
    const r = await call<{ id: string; client_id: string; created_client: boolean; status: Status }>("POST", "/accounts", a);
    await api.loadOverview();
    return r;
  },
  /** Changes saved details. Only what is sent replaces anything. */
  async updateAccount(id: string, a: { user_id: string; secrets: Record<string, string>; fields: Record<string, string> }) {
    const r = await call<{ status: Status }>("PUT", `/accounts/${id}`, a);
    await api.loadOverview();
    return r;
  },
  async setEnabled(id: string, enabled: boolean) {
    await call("POST", `/accounts/${id}/enabled`, { enabled });
    await api.loadOverview();
  },
  async deleteAccount(id: string) {
    await call("DELETE", `/accounts/${id}`);
    await api.loadOverview();
  },
};
