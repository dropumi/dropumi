# dropumi design

Design tokens and rules for the dropumi brand and app UI. The source of truth for the UI is `branding/dropumi-ui.pen`; the logo files are in `branding/final/` and the full logo rules are in `branding/GUIDELINES.md`.

## Brand

dropumi is free, open-source, ultra-lightweight, cross-platform local file sharing, an AirDrop-style experience that needs no internet. The look is friendly and minimal: a navy base, a sea-teal accent (*umi* = sea), and rounded geometric type.

The logo is a drop inside open ripple rings. The drop is navy and the ripples are teal. The wordmark is a custom `dropumi`: "drop" in navy and "umi" in teal. Always use the artwork files, never retype the wordmark in a font.

## Colour

### Brand colours (logo)

| Name | HEX | RGB | Use |
|---|---|---|---|
| Deep Navy | `#0B1F3A` | 11 · 31 · 58 | drop, "drop", dark brand surfaces |
| Umi Teal | `#0E94A6` | 14 · 148 · 166 | ripples, "umi" on light |
| Light Teal | `#3CC4D4` | 60 · 196 · 212 | ripples, "umi" on dark |
| White | `#FFFFFF` | 255 · 255 · 255 | drop on dark |

### UI tokens (light and dark)

These are the colour variables in `dropumi-ui.pen`. Use them by name, never the raw hex, so both themes stay in sync.

| Token | Light | Dark | Use |
|---|---|---|---|
| `bg` | `#F4F8FA` | `#07121F` | screen / window background |
| `surface` | `#FFFFFF` | `#0F2238` | cards, tab bar, dialogs, inputs |
| `surface-2` | `#E8F0F4` | `#162C45` | sidebar (Mac), segmented control, subtle fills |
| `text-primary` | `#0B1F3A` | `#F2F7FA` | titles and body text |
| `text-secondary` | `#5B6B7D` | `#9DB0C2` | captions, meta, inactive icons |
| `border` | `#DCE5EB` | `#1F3652` | dividers and outlines |
| `accent` | `#0E8A9B` | `#3CC4D4` | primary buttons, links, selected state |
| `accent-soft` | `#D6EEF2` | `#123A4C` | selected tab, tiles, "Send" pills, drop zone |
| `ring` | `#0E94A640` | `#3CC4D440` | radar rings, drop-zone outline (25% alpha) |
| `on-accent` | `#FFFFFF` | `#06111F` | text and icons on `accent` |
| `success` | `#1E9E6A` | `#4FD3A0` | visible dot, completed transfers, toggles |
| `danger` | `#C93B3B` | `#FF8080` | destructive actions (defined, not yet used) |
| `navy` | `#0B1F3A` | `#0B1F3A` | fixed brand navy, the same in both themes |

### Fixed colours (not themed)

Used only on the full-screen **Receive** screen on mobile (always navy) and on the macOS window buttons.

| Colour | HEX | Use |
|---|---|---|
| Receive card | `#12294A` | file list card on navy |
| Receive divider | `#25405F` | divider on navy |
| Receive muted text | `#9FB4C7` | secondary text on navy |
| Receive outline | `#3A5573` | Decline button outline |
| Traffic lights | `#FF5F57` `#FEBC2E` `#28C840` | macOS close / minimise / zoom |

### Contrast notes

- `accent` light (`#0E8A9B`) on white is about 4.1:1. Use it for bold or large text and controls, not for small body copy.
- `text-secondary` is for captions of 12 px and up on `bg` or `surface`.
- On dark, use `accent` (`#3CC4D4`) with `on-accent` (`#06111F`) text, which is high contrast.

## Typography

- **Font:** Plus Jakarta Sans (Google Fonts, SIL Open Font License) for both headings and body.
- **Variables:** `font-display` and `font-body` both resolve to Plus Jakarta Sans. To change the typeface (for example to Geist), change those two values and everything updates.
- **Fallback:** `system-ui, -apple-system, "Segoe UI", sans-serif`.
- **Status bar:** the OS draws it. It is shown as a design placeholder only.

| Role | Size | Weight | Notes |
|---|---|---|---|
| Screen title | 32 | 700 | the same size on every screen, mobile and desktop |
| Dialog name (Receive) | 26–30 | 700 | sender's device name |
| Section heading | 18 | 600 | "Tap a device to send", "Earlier today" |
| Wordmark (live text only) | 21–22 | 600 | "drop" in `text-primary`, "umi" in `accent`; use the logo artwork in production |
| Body / row title | 15–16 | 600 | file and device names |
| Button label | 13–16 | 600 | 16 on primary buttons, 13 on small buttons |
| Meta / caption | 12–13 | 400–500 | sizes, devices, times |
| Section label | 12 | 700 | uppercase, in `text-secondary` |
| Tab label | 10 | 500 | mobile tab bar |

## Shape, spacing and elevation

- **Radius:** full pill (half the height) for primary buttons and the tab bar, 28 px for the mobile tab bar capsule, 20–24 for cards and dialogs, 10–14 for tiles, segmented controls and small buttons, 12 (Mac) or 8 (Windows) for window corners.
- **Spacing:** 20 px screen padding on mobile, 40 px content padding on desktop. 20–24 px between sections, 12–14 px inside rows, 11–12 px row padding.
- **Borders:** 1 px `border`, drawn as a bottom-only divider for list rows.
- **Shadows:** soft and low. Cards use y 4–6, blur 12–20, navy at about 8–12% opacity. Dialogs use y 20, blur 60. Use no heavy gradients; the only gradient is the soft radial glow behind the radar.

## Layout

- **Mobile:** 390 × 844. A 62 px status bar, a content area with 20 px padding, and a floating capsule tab bar (Nearby, Transfers, Settings) 56 px tall, inset 16 px.
- **Desktop:** 1120 × 720 windows. A 232 px sidebar (Nearby, Transfers with a count badge, Settings, and this device at the bottom) next to the main area.
- **Mac:** traffic lights, with the sidebar running the full window height.
- **Windows:** a 36 px title bar with the app icon and minimise / maximise / close controls.

## Components

- **Radar:** your device at the centre (navy circle with the reversed logo), three concentric `ring` circles, and nearby devices as white avatar discs on the rings. It echoes the logo ripples.
- **Device row:** a tinted avatar disc, name and meta, and a "Send" pill (`accent-soft` fill, `accent` text).
- **File row:** a rounded tile with a type icon, name and size, and a trailing action.
- **Primary button:** `accent` fill, `on-accent` text, pill shape.
- **Segmented control:** a `surface-2` track with a `surface` selected segment.
- **Toggle:** 44 × 26 (desktop) or 48 × 28 (mobile); on = `accent`, off = `border`.
- **Progress bar:** an 8 px `surface-2` track with an `accent` fill.

## Icons

Lucide icons, 16–22 px, stroke style: `radar`, `arrow-left-right`, `settings`, `smartphone`, `laptop`, `monitor`, `upload`, `send`, `video`, `image`, `file-text`, `music`, `archive`, `circle-check`, `folder-down`, `shield-check`, `github`.

## Assets

| What | Where |
|---|---|
| Logo SVGs (colour, reversed, mono, small) | `branding/final/` |
| Favicon and web icons | `branding/final/web/` |
| iOS / Android icons, splash, store graphics | `branding/mobile/` |
| UI design file (Pencil) | `branding/dropumi-ui.pen` |
| UI exports (light and dark) | `branding/ui-export/` |
| Logo usage rules | `branding/GUIDELINES.md` |

## CSS variables

```css
:root {
  --bg: #F4F8FA;          --surface: #FFFFFF;     --surface-2: #E8F0F4;
  --text-primary: #0B1F3A; --text-secondary: #5B6B7D; --border: #DCE5EB;
  --accent: #0E8A9B;      --accent-soft: #D6EEF2; --ring: #0E94A640;
  --on-accent: #FFFFFF;   --success: #1E9E6A;     --danger: #C93B3B;
  --navy: #0B1F3A;
  --font: "Plus Jakarta Sans", system-ui, -apple-system, "Segoe UI", sans-serif;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #07121F;          --surface: #0F2238;     --surface-2: #162C45;
    --text-primary: #F2F7FA; --text-secondary: #9DB0C2; --border: #1F3652;
    --accent: #3CC4D4;      --accent-soft: #123A4C; --ring: #3CC4D440;
    --on-accent: #06111F;   --success: #4FD3A0;     --danger: #FF8080;
  }
}
```
