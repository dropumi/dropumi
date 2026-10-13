# dropumi: Logo Guidelines

## 1. The logo
- **Idea:** a drop at the centre of open ripples. Files land on nearby devices, with no internet in between. The rings are open at the top (open source) and the drop tip rises out of them.
- **Versions:** horizontal (primary) · stacked · symbol only · wordmark only. Every version also exists in black, white, navy one-colour and reversed (for dark backgrounds).
- **Wordmark:** lowercase `dropumi`, built from geometric strokes with round caps. "drop" is navy and "umi" is teal (*umi* = sea). Never retype it in a font; use the files.

## 2. Files (`branding/final/`)
| Need | File |
|---|---|
| Default, light background | `dropumi-horizontal.svg` · `dropumi-stacked.svg` · `dropumi-symbol.svg` · `dropumi-wordmark.svg` |
| Dark background | `*-reversed.svg` |
| One colour | `mono/*-black.svg` · `mono/*-white.svg` · `mono/*-mono-0b1f3a.svg` |
| Square avatar / social | `mono/dropumi-symbol-square.svg` |
| Small sizes (≤ 32 px) | `dropumi-symbol-small.svg` · `dropumi-symbol-small-reversed.svg` |
| Favicon, app icon, PWA | `web/`, with the `<head>` tags in `web/head-snippet.html` |

The SVGs use round-capped strokes. If a printer needs filled outlines, open the file in a vector editor and run *Expand / Outline Stroke* before sending.

## 3. Clear space
Keep a clear zone of **1 × the height of the letter "o"** (the x-height of the wordmark) on all sides. It scales with the logo.

## 4. Minimum size
| Version | Screen | Print |
|---|---|---|
| Horizontal | 120 px wide | 30 mm wide |
| Stacked | 80 px wide | 20 mm wide |
| Symbol | 32 px (use `-small` below that, down to 16 px) | 8 mm |

## 5. Colour
| Name | HEX | RGB | CMYK (approx.) | Use |
|---|---|---|---|---|
| Deep Navy | `#0B1F3A` | 11 · 31 · 58 | 81 · 46 · 0 · 77 | drop, "drop", dark backgrounds |
| Umi Teal | `#0E94A6` | 14 · 148 · 166 | 91 · 11 · 0 · 35 | ripples, "umi" |
| Light Teal | `#3CC4D4` | 60 · 196 · 212 | 72 · 8 · 0 · 17 | ripples and "umi" on dark only |
| White | `#FFFFFF` | 255 · 255 · 255 | 0 · 0 · 0 · 0 | drop on dark |

CMYK values are conversions, not matched swatches. Proof them on the real stock before a print run. No Pantone match has been chosen.

**Approved pairs:** full colour on white or very light grey · reversed on Deep Navy · black or white one-colour on anything calm. On photos, use the one-colour version on a quiet area.

## 6. Typography
Wordmark: custom, supplied as artwork. Suggested UI/body face: **Inter** (SIL Open Font License, free for commercial use), with a system sans-serif fallback.

## 7. Don'ts
Don't stretch or squash it · don't recolour outside the palette · don't rotate it · don't add shadows, outlines, gradients or effects · don't rearrange the symbol and wordmark · don't use the full symbol below 32 px (use the small cut) · don't place full colour on busy backgrounds · don't retype the wordmark.

## 8. Trademark note
This mark has not been searched against existing trademarks. Run a trademark and reverse-image search before relying on it commercially.
