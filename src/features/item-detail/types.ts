import type { BackpackItem, Tag } from "../backpack/api";
import type { TradeItemView } from "../trade-analyzer/api";

/**
 * Superset of the fields `BackpackItem` and the enriched `TradeItemView`
 * both carry — the Item Detail modal renders off of this shape so it
 * doesn't care which feature (Backpack, Trade Analyzer, Trade History)
 * opened it. Everything beyond `name` is nullable since a `TradeItemView`
 * for an unresolved trade item carries almost nothing.
 */
export interface DetailableItem {
  name: string;
  customLabel: string | null;
  quality: number | null;
  effectId: number | null;
  killstreakTier: number | null;
  australium: boolean | null;
  festivized: boolean | null;
  paintId: number | null;
  craftNumber: number | null;
  strangeCount: number | null;
  imageUrl: string | null;
  tradable: boolean | null;
  marketable: boolean | null;
  craftable: boolean | null;
  folder: string | null;
  tags: Tag[];
  note: string | null;
}

export function detailableFromBackpackItem(item: BackpackItem): DetailableItem {
  return {
    name: item.name,
    customLabel: item.meta.custom_label,
    quality: item.quality,
    effectId: item.effect_id,
    killstreakTier: item.killstreak_tier,
    australium: item.australium,
    festivized: item.festivized,
    paintId: item.paint_id,
    craftNumber: item.craft_number,
    strangeCount: item.strange_count,
    imageUrl: item.image_url,
    tradable: item.tradable,
    marketable: item.marketable,
    craftable: item.craftable,
    folder: item.meta.folder,
    tags: item.tags,
    note: item.meta.note,
  };
}

export function detailableFromTradeItemView(item: TradeItemView): DetailableItem {
  return {
    name: item.name,
    customLabel: null,
    quality: item.quality,
    effectId: item.effect_id,
    killstreakTier: item.killstreak_tier,
    australium: item.australium,
    festivized: item.festivized,
    paintId: item.paint_id,
    craftNumber: item.craft_number,
    strangeCount: item.strange_count,
    imageUrl: item.image_url,
    tradable: null,
    marketable: null,
    craftable: null,
    folder: null,
    tags: [],
    note: null,
  };
}

/**
 * Best-effort backpack.tf classifieds URL for the price section — `quality`
 * and `name` are the only fields `parse_classified_url` (Rust) actually
 * requires; every other query param is optional and simply isn't filtered
 * on when absent (`src-tauri/src/domain/classified_url.rs`), so a
 * `TradeItemView` missing tradable/craftable still resolves a URL, just a
 * less-filtered one. Returns `null` when there's not even a quality to
 * search on (a genuinely unresolved trade item).
 */
export function buildDetailClassifiedUrl(item: DetailableItem): string | null {
  if (item.quality === null) return null;
  const params = new URLSearchParams();
  params.set("item", item.name);
  params.set("quality", String(item.quality));
  if (item.effectId !== null) params.set("particle", String(item.effectId));
  if (item.tradable !== null) params.set("tradable", item.tradable ? "1" : "0");
  if (item.craftable !== null) params.set("craftable", item.craftable ? "1" : "0");
  if (item.australium !== null) params.set("australium", item.australium ? "1" : "0");
  if (item.killstreakTier !== null) params.set("killstreak_tier", String(item.killstreakTier));
  if (item.paintId !== null) params.set("paint", String(item.paintId));
  return `https://backpack.tf/classifieds?${params.toString()}`;
}
