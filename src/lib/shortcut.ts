import type { Platform } from "./types";

const MODIFIER_ORDER = ["Super", "Control", "Alt", "Shift"] as const;
type Modifier = (typeof MODIFIER_ORDER)[number];

const MAC_SYMBOLS: Record<Modifier, string> = { Super: "⌘", Control: "⌃", Alt: "⌥", Shift: "⇧" };
const PC_LABELS: Record<Modifier, string> = { Super: "Win", Control: "Ctrl", Alt: "Alt", Shift: "Shift" };

const KEY_LABELS: Record<string, string> = {
  Space: "Space",
  Enter: "↩",
  Tab: "⇥",
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
};

const MODIFIER_ALIASES: Record<string, Modifier> = {
  SUPER: "Super",
  CMD: "Super",
  COMMAND: "Super",
  CMDORCTRL: "Super",
  COMMANDORCONTROL: "Super",
  CONTROL: "Control",
  CTRL: "Control",
  ALT: "Alt",
  OPTION: "Alt",
  SHIFT: "Shift",
};

function keyLabel(code: string): string {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^[A-Z0-9]$/i.test(code)) return code.toUpperCase();
  return KEY_LABELS[code] ?? code;
}

/** Splits a stored accelerator ("Alt+Shift+KeyT") into display keycaps. */
export function shortcutKeys(accelerator: string, platform: Platform): string[] {
  const parts = accelerator.split("+").map((part) => part.trim()).filter(Boolean);
  const modifiers = new Set<Modifier>();
  let key = "";
  for (const part of parts) {
    const modifier = MODIFIER_ALIASES[part.toUpperCase()];
    if (modifier) modifiers.add(modifier);
    else key = part;
  }
  const labels = platform === "macos" ? MAC_SYMBOLS : PC_LABELS;
  const order: Modifier[] = platform === "macos" ? ["Control", "Alt", "Shift", "Super"] : [...MODIFIER_ORDER];
  return [...order.filter((modifier) => modifiers.has(modifier)).map((modifier) => labels[modifier]), keyLabel(key)].filter(Boolean);
}

const IGNORED_CODES = new Set([
  "MetaLeft",
  "MetaRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "ShiftLeft",
  "ShiftRight",
  "CapsLock",
  "Fn",
]);

export type RecordedShortcut =
  | { kind: "pending"; keys: string[] }
  | { kind: "invalid"; message: string }
  | { kind: "complete"; accelerator: string };

/**
 * Builds an accelerator from a keydown event. Uses `event.code` (physical key) because
 * `event.key` is layout- and Option-dependent on macOS (⌥T produces "†").
 */
export function recordShortcut(event: KeyboardEvent, platform: Platform): RecordedShortcut {
  const modifiers: Modifier[] = [];
  if (event.metaKey) modifiers.push("Super");
  if (event.ctrlKey) modifiers.push("Control");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");

  if (IGNORED_CODES.has(event.code) || !event.code) {
    return { kind: "pending", keys: shortcutKeys(modifiers.join("+"), platform) };
  }
  const isFunctionKey = /^F\d{1,2}$/.test(event.code);
  const hasPrimaryModifier = modifiers.some((modifier) => modifier !== "Shift");
  if (!isFunctionKey && !hasPrimaryModifier) {
    return { kind: "invalid", message: platform === "macos" ? "需要包含 ⌘、⌃ 或 ⌥" : "需要包含 Ctrl、Alt 或 Win" };
  }
  return { kind: "complete", accelerator: [...modifiers, event.code].join("+") };
}
