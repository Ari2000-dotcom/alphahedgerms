import type { JSX } from "preact";
import bold_arrow_right from "@phosphor-icons/core/assets/bold/arrow-right-bold.svg?raw";
import bold_arrows_clockwise from "@phosphor-icons/core/assets/bold/arrows-clockwise-bold.svg?raw";
import bold_caret_down from "@phosphor-icons/core/assets/bold/caret-down-bold.svg?raw";
import bold_caret_up from "@phosphor-icons/core/assets/bold/caret-up-bold.svg?raw";
import bold_check from "@phosphor-icons/core/assets/bold/check-bold.svg?raw";
import bold_download_simple from "@phosphor-icons/core/assets/bold/download-simple-bold.svg?raw";
import bold_eye from "@phosphor-icons/core/assets/bold/eye-bold.svg?raw";
import bold_eye_slash from "@phosphor-icons/core/assets/bold/eye-slash-bold.svg?raw";
import bold_key from "@phosphor-icons/core/assets/bold/key-bold.svg?raw";
import bold_lock from "@phosphor-icons/core/assets/bold/lock-bold.svg?raw";
import bold_magnifying_glass from "@phosphor-icons/core/assets/bold/magnifying-glass-bold.svg?raw";
import bold_pencil_simple from "@phosphor-icons/core/assets/bold/pencil-simple-bold.svg?raw";
import bold_plus from "@phosphor-icons/core/assets/bold/plus-bold.svg?raw";
import bold_sidebar_simple from "@phosphor-icons/core/assets/bold/sidebar-simple-bold.svg?raw";
import bold_sign_in from "@phosphor-icons/core/assets/bold/sign-in-bold.svg?raw";
import bold_sign_out from "@phosphor-icons/core/assets/bold/sign-out-bold.svg?raw";
import bold_trash from "@phosphor-icons/core/assets/bold/trash-bold.svg?raw";
import bold_x from "@phosphor-icons/core/assets/bold/x-bold.svg?raw";
import duotone_flow_arrow from "@phosphor-icons/core/assets/duotone/flow-arrow-duotone.svg?raw";
import duotone_gauge from "@phosphor-icons/core/assets/duotone/gauge-duotone.svg?raw";
import duotone_receipt from "@phosphor-icons/core/assets/duotone/receipt-duotone.svg?raw";
import duotone_shield_check from "@phosphor-icons/core/assets/duotone/shield-check-duotone.svg?raw";
import duotone_squares_four from "@phosphor-icons/core/assets/duotone/squares-four-duotone.svg?raw";
import duotone_stack from "@phosphor-icons/core/assets/duotone/stack-duotone.svg?raw";
import duotone_trend_up from "@phosphor-icons/core/assets/duotone/trend-up-duotone.svg?raw";
import duotone_user_check from "@phosphor-icons/core/assets/duotone/user-check-duotone.svg?raw";
import duotone_users from "@phosphor-icons/core/assets/duotone/users-duotone.svg?raw";
import duotone_wallet from "@phosphor-icons/core/assets/duotone/wallet-duotone.svg?raw";
import fill_flow_arrow from "@phosphor-icons/core/assets/fill/flow-arrow-fill.svg?raw";
import fill_receipt from "@phosphor-icons/core/assets/fill/receipt-fill.svg?raw";
import fill_shield_check from "@phosphor-icons/core/assets/fill/shield-check-fill.svg?raw";
import fill_squares_four from "@phosphor-icons/core/assets/fill/squares-four-fill.svg?raw";
import fill_stack from "@phosphor-icons/core/assets/fill/stack-fill.svg?raw";
import fill_users from "@phosphor-icons/core/assets/fill/users-fill.svg?raw";

// Icons: Phosphor. Section icons are duotone (a solid stroke over a soft fill) and turn fully solid
// when their page is open; actions are the bold weight so they hold their shape at 16px. Each
// export is a ready component; the names are the app's own.

const sized = (raw: string, size: number) => raw.replace("<svg ", `<svg width="${size}" height="${size}" aria-hidden="true" focusable="false" `);
const glyph = (raw: string, size: number) => () => <span class="pi" dangerouslySetInnerHTML={{ __html: sized(raw, size) }} />;
const section = (duo: string, solid: string, size = 20) => ({ on = false }: { on?: boolean }): JSX.Element => (
  <span class="pi" dangerouslySetInnerHTML={{ __html: sized(on ? solid : duo, size) }} />
);

export const NavOverview = section(duotone_squares_four, fill_squares_four);
export const NavAccounts = section(duotone_users, fill_users);
export const NavPositions = section(duotone_stack, fill_stack);
export const NavOrders = section(duotone_receipt, fill_receipt);
export const NavTrade = section(duotone_flow_arrow, fill_flow_arrow);
export const NavRisk = section(duotone_shield_check, fill_shield_check);

export const FigSignedIn = glyph(duotone_user_check, 18);
export const FigAvailable = glyph(duotone_wallet, 18);
export const FigUsed = glyph(duotone_gauge, 18);
export const FigOpen = glyph(duotone_stack, 18);
export const FigPnl = glyph(duotone_trend_up, 18);

export const Sidebar = glyph(bold_sidebar_simple, 20);
export const Refresh = glyph(bold_arrows_clockwise, 16);
export const Trash = glyph(bold_trash, 16);
export const Plus = glyph(bold_plus, 16);
export const Check = glyph(bold_check, 16);
export const X = glyph(bold_x, 16);
export const Search = glyph(bold_magnifying_glass, 16);
export const LogOut = glyph(bold_sign_out, 16);
export const LogIn = glyph(bold_sign_in, 16);
export const Lock = glyph(bold_lock, 16);
export const Eye = glyph(bold_eye, 18);
export const EyeOff = glyph(bold_eye_slash, 18);
export const ArrowNext = glyph(bold_arrow_right, 16);
export const ChevUp = glyph(bold_caret_up, 16);
export const ChevDown = glyph(bold_caret_down, 16);

/** The brand mark: an A whose crossbar is a hedge line. */
export const Mark = () => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M5.25 19.25 12 4.75l6.75 14.5" />
    <path d="M7.9 14h8.2" />
  </svg>
);
export const Pencil = glyph(bold_pencil_simple, 15);
export const Key = glyph(bold_key, 16);
export const Download = glyph(bold_download_simple, 15);
