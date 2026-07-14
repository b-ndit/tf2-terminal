// Quality ids match domain::item::Quality (src-tauri/src/domain/item.rs) —
// real Valve schema ids, stable since ~2011.
export const QUALITY_NAMES: Record<number, string> = {
  0: "Normal",
  1: "Genuine",
  2: "rarity2",
  3: "Vintage",
  4: "rarity3",
  5: "Unusual",
  6: "Unique",
  7: "Community",
  8: "Valve",
  9: "Self-Made",
  10: "Customized",
  11: "Strange",
  12: "Completed",
  13: "Haunted",
  14: "Collector's",
  15: "Decorated Weapon",
};

const QUALITY_COLOR_VARS: Record<number, string> = {
  0: "var(--color-quality-normal)",
  1: "var(--color-quality-genuine)",
  2: "var(--color-quality-normal)",
  3: "var(--color-quality-vintage)",
  4: "var(--color-quality-normal)",
  5: "var(--color-quality-unusual)",
  6: "var(--color-quality-unique)",
  7: "var(--color-quality-community)",
  8: "var(--color-quality-valve)",
  9: "var(--color-quality-selfmade)",
  10: "var(--color-quality-customized)",
  11: "var(--color-quality-strange)",
  12: "var(--color-quality-completed)",
  13: "var(--color-quality-haunted)",
  14: "var(--color-quality-collectors)",
  15: "var(--color-quality-decorated)",
};

export const KILLSTREAK_NAMES: Record<number, string> = {
  0: "",
  1: "Killstreak",
  2: "Specialized Killstreak",
  3: "Professional Killstreak",
};

export function qualityName(quality: number): string {
  return QUALITY_NAMES[quality] ?? `Quality ${quality}`;
}

export function qualityColor(quality: number): string {
  return QUALITY_COLOR_VARS[quality] ?? QUALITY_COLOR_VARS[0];
}

export function killstreakName(tier: number): string {
  return KILLSTREAK_NAMES[tier] ?? "";
}

/** Converts the raw RGB int stored in `paint_id` into a CSS hex color. */
export function paintToHex(paintRgb: number): string {
  return `#${(paintRgb & 0xffffff).toString(16).padStart(6, "0")}`;
}
