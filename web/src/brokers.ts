// What each broker needs to sign in, and where the admin finds each thing. Shown as hints
// while an account is being added. Same wording as the AlphaHedge desk.

export type BrokerId = "zerodha" | "angel" | "kotak" | "upstox" | "nubra";

export interface Field {
  key: string;
  label: string;
  /** Typed as dots, and sent to the server as a secret. */
  secret: boolean;
  /** Where to find it. */
  where: string;
}

export const BROKER_ORDER: BrokerId[] = ["zerodha", "kotak", "angel", "upstox", "nubra"];

export const BROKERS: Record<BrokerId, { name: string; note: string; fields: Field[]; positions: boolean }> = {
  zerodha: {
    name: "Zerodha",
    note: "The first time, approve the app once on Kite's Authorize page.",
    positions: true,
    fields: [
      { key: "userId", label: "User ID", secret: false, where: "Your Zerodha client ID, like AB1234." },
      { key: "apiKey", label: "API key", secret: true, where: "From your app on the Kite Connect developer console (developers.kite.trade)." },
      { key: "apiSecret", label: "API secret", secret: true, where: "Shown with the API key on the same app page." },
      { key: "password", label: "Web password", secret: true, where: "The password you use to log in to kite.zerodha.com." },
      { key: "totpSecret", label: "TOTP secret", secret: true, where: "On Kite, turn on External TOTP and copy the key under \"Can't scan? Copy key\". Not the 6-digit code." },
    ],
  },
  kotak: {
    name: "Kotak Neo",
    note: "Each Kotak user has their own consumer key and their own TOTP registration.",
    positions: true,
    fields: [
      { key: "userId", label: "UCC", secret: false, where: "The client's UCC, from their Neo profile." },
      { key: "apiKey", label: "Consumer key", secret: true, where: "The token on the client's own Neo API page. Every user has a different one." },
      { key: "pin", label: "MPIN", secret: true, where: "The client's 6-digit Neo MPIN." },
      { key: "totpSecret", label: "TOTP secret", secret: true, where: "The key shown when TOTP is registered in the Neo app. Finish by entering the first 6-digit code there." },
      { key: "mobile", label: "Mobile", secret: false, where: "The registered mobile with +91, like +919876543210." },
    ],
  },
  angel: {
    name: "Angel One",
    note: "Use the key from the client's SmartAPI app.",
    positions: true,
    fields: [
      { key: "userId", label: "Client code", secret: false, where: "The client's Angel One client code, like A123456." },
      { key: "apiKey", label: "API key", secret: true, where: "From the client's app on the SmartAPI portal (smartapi.angelone.in)." },
      { key: "pin", label: "PIN", secret: true, where: "The client's Angel One login PIN." },
      { key: "totpSecret", label: "TOTP secret", secret: true, where: "Enable TOTP on the SmartAPI portal and copy the secret key shown. Not the 6-digit code." },
    ],
  },
  upstox: {
    name: "Upstox",
    note: "The account must use TOTP, not SMS OTP, as its second factor. Positions are not read yet.",
    positions: false,
    fields: [
      { key: "apiKey", label: "API key", secret: true, where: "From the client's app on the Upstox developer console." },
      { key: "apiSecret", label: "API secret", secret: true, where: "Shown with the API key on the same app page." },
      { key: "pin", label: "PIN", secret: true, where: "The client's 6-digit Upstox PIN." },
      { key: "totpSecret", label: "TOTP secret", secret: true, where: "Turn on TOTP in the Upstox app and copy the secret key. SMS OTP will not work." },
      { key: "mobile", label: "Mobile", secret: false, where: "The mobile number on the client's Upstox account." },
      { key: "redirectUrl", label: "Redirect URL", secret: false, where: "Exactly as registered in the Upstox app, character for character." },
    ],
  },
  nubra: {
    name: "Nubra",
    note: "Turn on TOTP in the Nubra app first. Positions are not read yet.",
    positions: false,
    fields: [
      { key: "pin", label: "MPIN", secret: true, where: "The client's Nubra MPIN." },
      { key: "totpSecret", label: "TOTP secret", secret: true, where: "Turn on TOTP in the Nubra app and copy the secret key." },
      { key: "mobile", label: "Mobile", secret: false, where: "The mobile number on the client's Nubra account." },
    ],
  },
};
