import type { ThemeName } from "./types";

export interface AirwavTheme {
  name: ThemeName;
  bg: string;
  panel: string;
  text: string;
  muted: string;
  border: string;
  accent: string;
  prism: string;
  unknown: string;
  selected: string;
  danger: string;
  live: string;
  waterfall: string[];
}

const THEMES: Record<ThemeName, AirwavTheme> = {
  Midnight: {
    name: "Midnight",
    bg: "#0a0e16",
    panel: "#0f1620",
    text: "#d5e0ee",
    muted: "#75899f",
    border: "#2b3e53",
    accent: "#54bef0",
    prism: "#50d2bf",
    unknown: "#e6b562",
    selected: "#c5e8f7",
    danger: "#ee6582",
    live: "#50d2bf",
    waterfall: [
      "#0a0e16",
      "#101d31",
      "#1a2f52",
      "#274181",
      "#3669b0",
      "#46a1cc",
      "#7bc6db",
      "#d7eef6",
    ],
  },
  Radar: {
    name: "Radar",
    bg: "#061015",
    panel: "#0a181d",
    text: "#d0e5db",
    muted: "#6e8a80",
    border: "#1d3d38",
    accent: "#4ad0a6",
    prism: "#7ee0c0",
    unknown: "#e6b562",
    selected: "#d6fff0",
    danger: "#ee6582",
    live: "#4ad0a6",
    waterfall: [
      "#061015",
      "#07221c",
      "#0b3a30",
      "#0f5a46",
      "#1a8a68",
      "#4ad0a6",
      "#9ae8c8",
      "#e4faf0",
    ],
  },
  Arctic: {
    name: "Arctic",
    bg: "#dee6ef",
    panel: "#eaf0f6",
    text: "#172838",
    muted: "#5b7388",
    border: "#b7c6d4",
    accent: "#0864a7",
    prism: "#0b7a86",
    unknown: "#a15c12",
    selected: "#083d68",
    danger: "#b4233c",
    live: "#0b7a5a",
    waterfall: [
      "#dee6ef",
      "#c5d5e6",
      "#9bb8d6",
      "#6e96c4",
      "#3d74ae",
      "#1b5b96",
      "#0d3f72",
      "#082844",
    ],
  },
  Ember: {
    name: "Ember",
    bg: "#181015",
    panel: "#22161c",
    text: "#f0deda",
    muted: "#9a7d78",
    border: "#4a3036",
    accent: "#f4975d",
    prism: "#e8c07a",
    unknown: "#e6b562",
    selected: "#ffd8c2",
    danger: "#ee6582",
    live: "#78c484",
    waterfall: [
      "#181015",
      "#2a1416",
      "#4a1c1c",
      "#7a2a22",
      "#b4452c",
      "#e06a3a",
      "#f4975d",
      "#fde0c8",
    ],
  },
  Studio: {
    name: "Studio",
    bg: "#080c15",
    panel: "#0e1520",
    text: "#f3f7fc",
    muted: "#7b8da3",
    border: "#243044",
    accent: "#59d5f5",
    prism: "#7ee0d2",
    unknown: "#e6b562",
    selected: "#d7f6ff",
    danger: "#ee6582",
    live: "#50d2bf",
    waterfall: [
      "#080c15",
      "#0c1c33",
      "#123056",
      "#1a4a82",
      "#2a74b8",
      "#59d5f5",
      "#a6eaf8",
      "#e8f8ff",
    ],
  },
};

export function getTheme(name: ThemeName): AirwavTheme {
  return THEMES[name];
}

export function hexToRgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function waterfallLut(theme: AirwavTheme): Uint8ClampedArray {
  const lut = new Uint8ClampedArray(256 * 4);
  const stops = theme.waterfall.map(hexToRgb);
  for (let i = 0; i < 256; i++) {
    const t = (i / 255) * (stops.length - 1);
    const a = Math.floor(t);
    const b = Math.min(stops.length - 1, a + 1);
    const f = t - a;
    const ca = stops[a]!;
    const cb = stops[b]!;
    lut[i * 4] = Math.round(ca[0] + (cb[0] - ca[0]) * f);
    lut[i * 4 + 1] = Math.round(ca[1] + (cb[1] - ca[1]) * f);
    lut[i * 4 + 2] = Math.round(ca[2] + (cb[2] - ca[2]) * f);
    lut[i * 4 + 3] = 255;
  }
  return lut;
}
