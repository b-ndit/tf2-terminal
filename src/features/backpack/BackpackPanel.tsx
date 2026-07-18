import { useCallback, type ReactNode } from "react";
import { openOrFocusPanel } from "../../app/workspace/dockviewApi";
import { PANEL_TITLES } from "../../stores/workspaceStore";
import { useMarketAnalyzerStore } from "../../stores/marketAnalyzerStore";
import { buildClassifiedUrl, useInventory, type BackpackItem } from "./api";
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
  const setPendingUrl = useMarketAnalyzerStore((s) => s.setPendingUrl);

  const openAnalysis = useCallback(
    (item: BackpackItem) => {
      setPendingUrl(buildClassifiedUrl(item));
      openOrFocusPanel("market-analyzer", PANEL_TITLES["market-analyzer"]);
    },
    [setPendingUrl],
  );

  return (
    <div className="flex h-full min-h-0 flex-col">
      {error && <p className="px-4 py-1 text-sm text-red-400">{error.message}</p>}
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

function PanelMessage({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-full flex-col items-center justify-center text-center text-fg-muted">{children}</div>
  );
}
