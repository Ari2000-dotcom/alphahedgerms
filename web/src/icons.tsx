import type { ComponentChildren } from "preact";

// AlphaHedge Desk icon set, drawn for this app.
//
// Every icon sits on one 24-unit grid with a 1.5 stroke, round caps and round
// joins, and 2-unit corner radii. Drawn at 16px that stroke lands on exactly
// 1 device pixel, and at 150 % or 200 % scaling (4K) it stays a clean 1.5 or
// 2 pixels, so nothing looks soft.
//
// The section icons in the side bar carry a second layer (class "tone"): a
// filled shape the CSS fades in when the section is active or hovered, so the
// active one reads as solid without changing its outline.

function Svg({ children, size = 16 }: { children: ComponentChildren; size?: number }) {
  return (
    <svg
      width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"
      stroke-linecap="round" stroke-linejoin="round" shape-rendering="geometricPrecision" aria-hidden="true"
    >
      {children}
    </svg>
  );
}

const NAV = 18;

// ------------------------------------------------------------------ side bar

/** Trade: buy and sell, one arrow up and one down. */
export const NavTrade = () => (
  <Svg size={NAV}>
    <rect class="tone" x="3" y="3" width="18" height="18" rx="5" />
    <path d="M8.5 18V6M5.5 9 8.5 6l3 3" />
    <path d="M15.5 6v12M12.5 15l3 3 3-3" />
  </Svg>
);

/** Orders: an order slip. */
export const NavOrders = () => (
  <Svg size={NAV}>
    <rect class="tone" x="5" y="3.5" width="14" height="17" rx="2.5" />
    <rect x="5" y="3.5" width="14" height="17" rx="2.5" />
    <path d="M9 8.5h6M9 12h6M9 15.5h3.5" />
  </Svg>
);

/** Positions: stacked holdings. */
export const NavPositions = () => (
  <Svg size={NAV}>
    <path class="tone" d="M12 3.75 20.25 8 12 12.25 3.75 8Z" />
    <path d="M12 3.75 20.25 8 12 12.25 3.75 8Z" />
    <path d="M3.75 12 12 16.25 20.25 12" />
    <path d="M3.75 16 12 20.25 20.25 16" />
  </Svg>
);

/** Accounts: the people whose accounts these are. */
export const NavAccounts = () => (
  <Svg size={NAV}>
    <circle class="tone" cx="9" cy="8.5" r="3.5" />
    <circle cx="9" cy="8.5" r="3.5" />
    <path d="M2.75 19.5c.7-3.3 3.1-5.25 6.25-5.25s5.55 1.95 6.25 5.25" />
    <path d="M15.5 5.25a3.5 3.5 0 0 1 0 6.5" />
    <path d="M17.75 14.4c1.75.75 3 2.4 3.5 5.1" />
  </Svg>
);

/** Risk: a shield with a tick. */
export const NavRisk = () => (
  <Svg size={NAV}>
    <path class="tone" d="M12 3.25 19.25 6v5.5c0 4.4-3 7.9-7.25 9.25C7.75 19.4 4.75 15.9 4.75 11.5V6Z" />
    <path d="M12 3.25 19.25 6v5.5c0 4.4-3 7.9-7.25 9.25C7.75 19.4 4.75 15.9 4.75 11.5V6Z" />
    <path d="m9 12 2.1 2.1L15 10.2" />
  </Svg>
);

/** Alerts: a bell. */
export const NavAlerts = () => (
  <Svg size={NAV}>
    <path class="tone" d="M6.25 16.25V11a5.75 5.75 0 0 1 11.5 0v5.25l1.5 2H4.75Z" />
    <path d="M6.25 16.25V11a5.75 5.75 0 0 1 11.5 0v5.25l1.5 2H4.75Z" />
    <path d="M10 21h4" />
  </Svg>
);

/** Settings: two sliders. */
export const NavSettings = () => (
  <Svg size={NAV}>
    <circle class="tone" cx="15.5" cy="7.5" r="2.25" />
    <circle class="tone" cx="8.5" cy="16.5" r="2.25" />
    <path d="M4 7.5h9.25M17.75 7.5H20M4 16.5h2.25M10.75 16.5H20" />
    <circle cx="15.5" cy="7.5" r="2.25" />
    <circle cx="8.5" cy="16.5" r="2.25" />
  </Svg>
);

// ------------------------------------------------------------------ actions

export const Refresh = () => (
  <Svg>
    <path d="M19.75 12a7.75 7.75 0 1 1-2.27-5.48" />
    <path d="M19.75 4.25V8.5H15.5" />
  </Svg>
);
export const Trash = () => (
  <Svg>
    <path d="M4.5 6.5h15" />
    <path d="M9.5 6.5V4.75c0-.7.55-1.25 1.25-1.25h2.5c.7 0 1.25.55 1.25 1.25V6.5" />
    <path d="m6.5 6.5.8 12.2c.07 1 .9 1.8 1.9 1.8h5.6c1 0 1.83-.8 1.9-1.8l.8-12.2" />
    <path d="M10 10.5v6M14 10.5v6" />
  </Svg>
);
export const Plus = () => (<Svg><path d="M12 5.25v13.5M5.25 12h13.5" /></Svg>);
export const Check = () => (<Svg><path d="m5.25 12.5 4.25 4.25 9.25-9.5" /></Svg>);
export const X = () => (<Svg><path d="m6.75 6.75 10.5 10.5M17.25 6.75l-10.5 10.5" /></Svg>);
export const Search = () => (<Svg><circle cx="10.75" cy="10.75" r="6.25" /><path d="m15.5 15.5 4.25 4.25" /></Svg>);
export const Copy = () => (
  <Svg size={13}>
    <rect x="8.75" y="8.75" width="11" height="11" rx="2.25" />
    <path d="M15.25 8.75V6.5c0-.97-.78-1.75-1.75-1.75H6.5c-.97 0-1.75.78-1.75 1.75v7c0 .97.78 1.75 1.75 1.75h2.25" />
  </Svg>
);
export const Funnel = ({ size = 16 }: { size?: number }) => (<Svg size={size}><path d="M4.25 5.25h15.5l-5.9 7v5.1l-3.7 1.9v-7Z" /></Svg>);
export const Eye = () => (
  <Svg>
    <path d="M2.75 12S6.25 5.75 12 5.75 21.25 12 21.25 12 17.75 18.25 12 18.25 2.75 12 2.75 12Z" />
    <circle cx="12" cy="12" r="2.75" />
  </Svg>
);
export const EyeOff = () => (
  <Svg>
    <path d="m3.75 3.75 16.5 16.5" />
    <path d="M10.5 5.9a8.6 8.6 0 0 1 1.5-.15c5.75 0 9.25 6.25 9.25 6.25a16 16 0 0 1-2.75 3.45" />
    <path d="M6.6 7.55C4 9.25 2.75 12 2.75 12s3.5 6.25 9.25 6.25c1.55 0 2.95-.45 4.15-1.1" />
    <path d="M10.05 10.05a2.75 2.75 0 0 0 3.9 3.9" />
  </Svg>
);
export const ChevUp = () => (<Svg size={12}><path d="m7 14.5 5-5 5 5" /></Svg>);
export const ChevDown = () => (<Svg size={12}><path d="m7 9.5 5 5 5-5" /></Svg>);
/** Both directions, shown faintly on a sortable heading that is not sorted yet. */
export const ChevSort = () => (<Svg size={12}><path d="m8 9.5 4-4 4 4M8 14.5l4 4 4-4" /></Svg>);

// ------------------------------------------------------------------ theme

export const Moon = () => (<Svg><path d="M19.75 14.4A7.75 7.75 0 0 1 9.6 4.25a7.75 7.75 0 1 0 10.15 10.15Z" /></Svg>);
export const Sun = () => (
  <Svg>
    <circle cx="12" cy="12" r="3.75" />
    <path d="M12 3v1.75M12 19.25V21M3 12h1.75M19.25 12H21M5.64 5.64l1.24 1.24M17.12 17.12l1.24 1.24M18.36 5.64l-1.24 1.24M6.88 17.12l-1.24 1.24" />
  </Svg>
);
export const Monitor = () => (<Svg><rect x="3.25" y="4.5" width="17.5" height="12" rx="2.25" /><path d="M9 20h6M12 16.5V20" /></Svg>);

// ------------------------------------------------------------------ brand

/** The AlphaHedge mark: an A whose crossbar is a hedge line. */
export const Mark = () => (<Svg size={15}><path d="M5.25 19.25 12 4.75l6.75 14.5" /><path d="M7.9 14h8.2" /></Svg>);
export const NavOverview = () => (<Svg size={18}><rect class="tone" x="3.5" y="3.5" width="7" height="7" rx="2" /><rect x="3.5" y="3.5" width="7" height="7" rx="2" /><rect x="13.5" y="3.5" width="7" height="7" rx="2" /><rect x="3.5" y="13.5" width="7" height="7" rx="2" /><rect x="13.5" y="13.5" width="7" height="7" rx="2" /></Svg>);
