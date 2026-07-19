import { useCallback, useMemo, useState, type ReactNode } from "react";
import { useItemDetailStore } from "../../stores/itemDetailStore";
import { useSyncItemSchema } from "../settings/api";
import { detailableFromBackpackItem } from "../item-detail/types";
import { useInventory, type BackpackItem } from "./api";
import { BackpackGrid } from "./BackpackGrid";
import { ContextMenu } from "./ContextMenu";
import { StatsBar } from "./StatsBar";

/**
 * Composes the backpack tab's pieces into one self-sufficient Dockview
 * panel (own query, no props from the shell) — this is what used to be
 * the "backpack" branch inline in App.tsx's tab ternary.
 */
export function BackpackPanel() {
  const { data: items = [], isLoading, error } = useInventory();
  const openItemDetail = useItemDetailStore((s) => s.open);
  const [nudgeDismissed, setNudgeDismissed] = useState(false);

  // A plain click opens the Item Detail modal (icon/badges/embedded price
  // chart) rather than jumping straight to the Market Analyzer panel —
  // that full panel view is still one click away via the modal's own
  // "View in Market Analyzer" button.
  const openAnalysis = useCallback(
    (item: BackpackItem) => {
      openItemDetail(detailableFromBackpackItem(item));
    },
    [openItemDetail],
  );

  // Item names/icons only come from a schema sync (Settings → "Sync Item
  // Schema") — without it, items show as "Unknown Item {defindex}" with
  // no icon. This is easy to miss since it's a separate manual step from
  // "Sync Inventory", so nudge toward it right where the gap is visible.
  const unresolvedCount = useMemo(
    () => items.filter((item) => item.name.startsWith("Unknown Item ") || item.image_url === null).length,
    [items],
  );

  return (
    <div className="flex h-full min-h-0 flex-col">
      {error && <p className="px-4 py-1 text-sm text-red-400">{error.message}</p>}
      {!nudgeDismissed && unresolvedCount > 0 && (
        <SchemaSyncNudge count={unresolvedCount} onDismiss={() => setNudgeDismissed(true)} />
      )}
      <StatsBar items={items} />
      <div className="min-h-0 flex-1">
        {isLoading ? (
          <PanelMessage>Loading backpack…</PanelMessage>
        ) : items.length === 0 ? (
          <PanelMessage>No items synced yet. Click "Sync Inventory" to fetch your backpack.</PanelMessage>
        ) : (
          <BackpackGrid items={items} onOpenAnalysis={openAnalysis} />
        )}
      </div>
      <ContextMenu items={items} />
    </div>
  );
}

function SchemaSyncNudge({ count, onDismiss }: { count: number; onDismiss: () => void }) {
  const sync = useSyncItemSchema();

  return (
    <div className="mx-4 mt-3 flex flex-wrap items-center justify-between gap-2 rounded border border-amber-900 bg-amber-950/30 px-3 py-2 text-sm text-amber-300">
      <span>
        {count} item{count === 1 ? "" : "s"} missing a name or icon — sync the item schema to fetch them.
      </span>
      <div className="flex items-center gap-2">
        <button
          type="button"
          disabled={sync.isPending}
          onClick={() => sync.mutate(undefined, { onSuccess: onDismiss })}
          className="rounded bg-quality-unique px-2 py-1 text-xs font-medium text-black hover:opacity-90 disabled:opacity-50"
        >
          {sync.isPending ? "Syncing…" : "Sync Item Schema"}
        </button>
        <button
          type="button"
          onClick={onDismiss}
          className="rounded px-2 py-1 text-xs text-amber-300 hover:bg-amber-900/40"
        >
          Dismiss
        </button>
      </div>
      {sync.isError && <p className="w-full text-xs text-red-400">{sync.error.message}</p>}
    </div>
  );
}

function PanelMessage({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-full flex-col items-center justify-center text-center text-fg-muted">{children}</div>
  );
}
