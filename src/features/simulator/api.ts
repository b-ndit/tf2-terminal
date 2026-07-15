import { useQuery } from "@tanstack/react-query";
import { commands, type ItemKeyInput, type ItemSearchResult, type SimulatedTradeView } from "../../lib/bindings";

async function unwrap<T>(promise: Promise<{ status: "ok"; data: T } | { status: "error"; error: unknown }>): Promise<T> {
  const result = await promise;
  if (result.status === "error") {
    const message =
      typeof result.error === "object" && result.error !== null && "message" in result.error
        ? String((result.error as { message: unknown }).message)
        : String(result.error);
    throw new Error(message);
  }
  return result.data;
}

export interface SearchFilters {
  name: string;
  quality: number | null;
  killstreakTier: number | null;
  australium: boolean | null;
  craftable: boolean | null;
  hasEffect: boolean | null;
}

const EMPTY_FILTERS: SearchFilters = {
  name: "",
  quality: null,
  killstreakTier: null,
  australium: null,
  craftable: null,
  hasEffect: null,
};

export function hasAnyFilter(filters: SearchFilters): boolean {
  return (
    filters.name.trim() !== "" ||
    filters.quality !== null ||
    filters.killstreakTier !== null ||
    filters.australium !== null ||
    filters.craftable !== null ||
    filters.hasEffect !== null
  );
}

export { EMPTY_FILTERS };

// Search returns catalog data only (no live pricing) — cheap enough to
// query on every filter change; valuation happens once, when an item is
// actually added to a bucket (see useSimulateTrade below).
export function useSearchItems(filters: SearchFilters) {
  const enabled = hasAnyFilter(filters);
  return useQuery({
    queryKey: ["item-search", filters],
    queryFn: () =>
      unwrap(
        commands.searchItems(
          filters.name.trim() === "" ? null : filters.name.trim(),
          filters.quality,
          filters.killstreakTier,
          filters.australium,
          filters.craftable,
          filters.hasEffect,
        ),
      ),
    enabled,
  });
}

// Recomputes whenever either bucket's contents change (the query key
// captures both) — no backend persistence, same "recompute fresh"
// shape as Trade Analyzer/Flip Finder.
export function useSimulateTrade(givenAssetIds: string[], receivedKeys: ItemKeyInput[]) {
  return useQuery({
    queryKey: ["simulate-trade", givenAssetIds, receivedKeys],
    queryFn: () => unwrap(commands.simulateTrade(givenAssetIds, receivedKeys)),
    enabled: givenAssetIds.length > 0 || receivedKeys.length > 0,
  });
}

export type { ItemKeyInput, ItemSearchResult, SimulatedTradeView };
