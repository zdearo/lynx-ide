import type { ITheme } from "@xterm/xterm";

import type { ColorScheme, TerminalConfig } from "@/bindings";

/** One theme: its background and text colour, and the eight normal and eight bright ANSI colours. */
interface Scheme {
  background: string;
  foreground: string;
  normal: readonly [
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
  ];
  bright: readonly [
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
  ];
}

const ANSI_NAMES = [
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
] as const;

/** The palette a theme gives a terminal: the ANSI colours named the way xterm expects them. */
function toTheme(scheme: Scheme): ResolvedTheme {
  const ansi: Partial<Record<string, string>> = {};
  ANSI_NAMES.forEach((name, index) => {
    ansi[name] = scheme.normal[index];
    ansi[`bright${name[0].toUpperCase()}${name.slice(1)}`] =
      scheme.bright[index];
  });
  return {
    ...ansi,
    background: scheme.background,
    foreground: scheme.foreground,
  };
}

// The default theme keeps the engine's own ANSI colours, so it looks as it always did.
const SCHEMES: Record<ColorScheme, Scheme> = {
  default: {
    background: "#0a0a0a",
    foreground: "#fafafa",
    normal: [
      "#2e3436",
      "#cc0000",
      "#4e9a06",
      "#c4a000",
      "#3465a4",
      "#75507b",
      "#06989a",
      "#d3d7cf",
    ],
    bright: [
      "#555753",
      "#ef2929",
      "#8ae234",
      "#fce94f",
      "#729fcf",
      "#ad7fa8",
      "#34e2e2",
      "#eeeeec",
    ],
  },
  light: {
    background: "#fafafa",
    foreground: "#1f2328",
    normal: [
      "#24292f",
      "#cf222e",
      "#116329",
      "#4d2d00",
      "#0969da",
      "#8250df",
      "#1b7c83",
      "#6e7781",
    ],
    bright: [
      "#57606a",
      "#a40e26",
      "#1a7f37",
      "#633c01",
      "#218bff",
      "#a475f9",
      "#3192aa",
      "#8c959f",
    ],
  },
  solarizedDark: {
    background: "#002b36",
    foreground: "#839496",
    normal: [
      "#073642",
      "#dc322f",
      "#859900",
      "#b58900",
      "#268bd2",
      "#d33682",
      "#2aa198",
      "#eee8d5",
    ],
    bright: [
      "#002b36",
      "#cb4b16",
      "#586e75",
      "#657b83",
      "#839496",
      "#6c71c4",
      "#93a1a1",
      "#fdf6e3",
    ],
  },
  solarizedLight: {
    background: "#fdf6e3",
    foreground: "#657b83",
    normal: [
      "#073642",
      "#dc322f",
      "#859900",
      "#b58900",
      "#268bd2",
      "#d33682",
      "#2aa198",
      "#eee8d5",
    ],
    bright: [
      "#002b36",
      "#cb4b16",
      "#586e75",
      "#657b83",
      "#839496",
      "#6c71c4",
      "#93a1a1",
      "#fdf6e3",
    ],
  },
  dracula: {
    background: "#282a36",
    foreground: "#f8f8f2",
    normal: [
      "#21222c",
      "#ff5555",
      "#50fa7b",
      "#f1fa8c",
      "#bd93f9",
      "#ff79c6",
      "#8be9fd",
      "#f8f8f2",
    ],
    bright: [
      "#6272a4",
      "#ff6e6e",
      "#69ff94",
      "#ffffa5",
      "#d6acff",
      "#ff92df",
      "#a4ffff",
      "#ffffff",
    ],
  },
  nord: {
    background: "#2e3440",
    foreground: "#d8dee9",
    normal: [
      "#3b4252",
      "#bf616a",
      "#a3be8c",
      "#ebcb8b",
      "#81a1c1",
      "#b48ead",
      "#88c0d0",
      "#e5e9f0",
    ],
    bright: [
      "#4c566a",
      "#bf616a",
      "#a3be8c",
      "#ebcb8b",
      "#81a1c1",
      "#b48ead",
      "#8fbcbb",
      "#eceff4",
    ],
  },
  catppuccinLatte: {
    background: "#eff1f5",
    foreground: "#4c4f69",
    normal: [
      "#5c5f77",
      "#d20f39",
      "#40a02b",
      "#df8e1d",
      "#1e66f5",
      "#ea76cb",
      "#179299",
      "#acb0be",
    ],
    bright: [
      "#6c6f85",
      "#d20f39",
      "#40a02b",
      "#df8e1d",
      "#1e66f5",
      "#ea76cb",
      "#179299",
      "#bcc0cc",
    ],
  },
  catppuccinFrappe: {
    background: "#303446",
    foreground: "#c6d0f5",
    normal: [
      "#51576d",
      "#e78284",
      "#a6d189",
      "#e5c890",
      "#8caaee",
      "#f4b8e4",
      "#81c8be",
      "#b5bfe2",
    ],
    bright: [
      "#626880",
      "#e78284",
      "#a6d189",
      "#e5c890",
      "#8caaee",
      "#f4b8e4",
      "#81c8be",
      "#a5adce",
    ],
  },
  catppuccinMacchiato: {
    background: "#24273a",
    foreground: "#cad3f5",
    normal: [
      "#494d64",
      "#ed8796",
      "#a6da95",
      "#eed49f",
      "#8aadf4",
      "#f5bde6",
      "#8bd5ca",
      "#b8c0e0",
    ],
    bright: [
      "#5b6078",
      "#ed8796",
      "#a6da95",
      "#eed49f",
      "#8aadf4",
      "#f5bde6",
      "#8bd5ca",
      "#a5adcb",
    ],
  },
  catppuccinMocha: {
    background: "#1e1e2e",
    foreground: "#cdd6f4",
    normal: [
      "#45475a",
      "#f38ba8",
      "#a6e3a1",
      "#f9e2af",
      "#89b4fa",
      "#f5c2e7",
      "#94e2d5",
      "#bac2de",
    ],
    bright: [
      "#585b70",
      "#f38ba8",
      "#a6e3a1",
      "#f9e2af",
      "#89b4fa",
      "#f5c2e7",
      "#94e2d5",
      "#a6adc8",
    ],
  },
  gruvboxDark: {
    background: "#282828",
    foreground: "#ebdbb2",
    normal: [
      "#282828",
      "#cc241d",
      "#98971a",
      "#d79921",
      "#458588",
      "#b16286",
      "#689d6a",
      "#a89984",
    ],
    bright: [
      "#928374",
      "#fb4934",
      "#b8bb26",
      "#fabd2f",
      "#83a598",
      "#d3869b",
      "#8ec07c",
      "#ebdbb2",
    ],
  },
  oneDark: {
    background: "#282c34",
    foreground: "#abb2bf",
    normal: [
      "#3f4451",
      "#e06c75",
      "#98c379",
      "#e5c07b",
      "#61afef",
      "#c678dd",
      "#56b6c2",
      "#abb2bf",
    ],
    bright: [
      "#4f5666",
      "#be5046",
      "#98c379",
      "#d19a66",
      "#61afef",
      "#c678dd",
      "#56b6c2",
      "#ffffff",
    ],
  },
  tokyoNight: {
    background: "#1a1b26",
    foreground: "#c0caf5",
    normal: [
      "#15161e",
      "#f7768e",
      "#9ece6a",
      "#e0af68",
      "#7aa2f7",
      "#bb9af7",
      "#7dcfff",
      "#a9b1d6",
    ],
    bright: [
      "#414868",
      "#f7768e",
      "#9ece6a",
      "#e0af68",
      "#7aa2f7",
      "#bb9af7",
      "#7dcfff",
      "#c0caf5",
    ],
  },
  monokai: {
    background: "#272822",
    foreground: "#f8f8f2",
    normal: [
      "#272822",
      "#f92672",
      "#a6e22e",
      "#f4bf75",
      "#66d9ef",
      "#ae81ff",
      "#a1efe4",
      "#f8f8f2",
    ],
    bright: [
      "#75715e",
      "#f92672",
      "#a6e22e",
      "#f4bf75",
      "#66d9ef",
      "#ae81ff",
      "#a1efe4",
      "#f9f8f5",
    ],
  },
};

export type ResolvedTheme = ITheme & { background: string; foreground: string };

export function isColourScheme(value: string): value is ColorScheme {
  return Object.hasOwn(SCHEMES, value);
}

/** Light backgrounds need dark scrollbars, so the scrollbar follows the background's brightness. */
function isLight(hex: string): boolean {
  const value = Number.parseInt(hex.slice(1), 16);
  const red = (value >> 16) & 0xff;
  const green = (value >> 8) & 0xff;
  const blue = value & 0xff;
  return (0.299 * red + 0.587 * green + 0.114 * blue) / 255 > 0.5;
}

/**
 * The palette the terminal uses: the background of the chosen theme, and the text
 * palette (text colour and ANSI colours) of the text theme, which is the same theme
 * unless another one was chosen.
 */
export function resolveTerminalTheme(config: TerminalConfig): ResolvedTheme {
  const theme = SCHEMES[config.colorScheme];
  const text = SCHEMES[config.textColorScheme ?? config.colorScheme];
  const light = isLight(theme.background);
  return {
    ...toTheme(text),
    background: theme.background,
    // The cursor follows the text, so it stays visible on any background.
    cursor: text.foreground,
    scrollbarSliderBackground: light
      ? "rgba(0, 0, 0, 0.18)"
      : "rgba(255, 255, 255, 0.10)",
    scrollbarSliderHoverBackground: light
      ? "rgba(0, 0, 0, 0.28)"
      : "rgba(255, 255, 255, 0.18)",
    scrollbarSliderActiveBackground: light
      ? "rgba(0, 0, 0, 0.36)"
      : "rgba(255, 255, 255, 0.26)",
  };
}

/** The six normal colours a prompt uses, in the order a swatch row shows them. */
export function previewColours(theme: ITheme): string[] {
  return [
    theme.red,
    theme.green,
    theme.yellow,
    theme.blue,
    theme.magenta,
    theme.cyan,
  ].filter((colour): colour is string => colour !== undefined);
}
