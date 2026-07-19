import { useEffect, useState } from "react";
import { openOrFocusPanel } from "../../app/workspace/dockviewApi";
import { useItemDetailStore } from "../../stores/itemDetailStore";
import { useMarketAnalyzerStore } from "../../stores/marketAnalyzerStore";
import { PANEL_TITLES } from "../../stores/workspaceStore";
import { ItemIcon } from "../backpack/ItemIcon";
import { killstreakName, paintToHex, qualityColor, qualityName } from "../backpack/quality";
import { useAnalyzeClassifiedUrl } from "../market-analyzer/api";
import { PriceChart } from "../market-analyzer/PriceChart";
import { buildDetailClassifiedUrl } from "./types";

/**
 * The first modal/backdrop overlay in this codebase (everything else is
 * either a hover tooltip or a `fixed`-positioned menu) — mounted once near
 * the app root and driven entirely by `itemDetailStore`, so any feature
 * can open it for any item without prop drilling (same imperative-escape-
 * hatch shape `app/workspace/dockviewApi.ts` uses for panels).
 */
export function ItemDetailModal() {
  const item = useItemDetailStore((s) => s.item);
  const close = useItemDetailStore((s) => s.close);
  const setPendingUrl = useMarketAnalyzerStore((s) => s.setPendingUrl);
  const analyze = useAnalyzeClassifiedUrl();
  const [resolvedUrl, setResolvedUrl] = useState<string | null>(null);

  const candidateUrl = item ? buildDetailClassifiedUrl(item) : null;

  useEffect(() => {
    setResolvedUrl(null);
    if (!candidateUrl) return;
    analyze.mutate(candidateUrl, { onSuccess: () => setResolvedUrl(candidateUrl) });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [candidateUrl]);

  useEffect(() => {
    if (!item) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") close();
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [item, close]);

  if (!item) return null;

  function openInMarketAnalyzer() {
    if (!candidateUrl) return;
    setPendingUrl(candidateUrl);
    openOrFocusPanel("market-analyzer", PANEL_TITLES["market-analyzer"]);
    close();
  }

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 p-4"
      onClick={close}
    >
      <div
        className="max-h-[85vh] w-full max-w-md overflow-y-auto rounded-lg border border-charcoal-border bg-charcoal-raised p-4 text-fg shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-3 flex items-start justify-between gap-2">
          <div className="flex items-center gap-3">
            <ItemIcon imageUrl={item.imageUrl} alt={item.name} size="h-14 w-14" />
            <div>
              <p
                className="font-semibold"
                style={item.quality !== null ? { color: qualityColor(item.quality) } : undefined}
              >
                {item.quality !== null ? `${qualityName(item.quality)} ` : ""}
                {item.name}
              </p>
              {item.customLabel && <p className="text-sm text-fg-muted">"{item.customLabel}"</p>}
            </div>
          </div>
          <button
            type="button"
            onClick={close}
            aria-label="Close"
            className="rounded px-2 py-1 text-fg-muted hover:bg-charcoal-border"
          >
            ✕
          </button>
        </div>

        <div className="flex flex-wrap gap-x-4 gap-y-1 text-sm text-fg-muted">
          {item.killstreakTier !== null && item.killstreakTier > 0 && <span>{killstreakName(item.killstreakTier)}</span>}
          {item.australium && <span>Australium</span>}
          {item.festivized && <span>Festivized</span>}
          {item.strangeCount !== null && <span>Strange count: {item.strangeCount}</span>}
          {item.craftNumber !== null && <span>Craft #{item.craftNumber}</span>}
          {item.paintId !== null && (
            <span className="flex items-center gap-1">
              Paint
              <span
                className="inline-block h-2.5 w-2.5 rounded-full border border-black/40"
                style={{ backgroundColor: paintToHex(item.paintId) }}
              />
            </span>
          )}
          {item.tradable !== null && <span>{item.tradable ? "Tradable" : "Not tradable"}</span>}
          {item.marketable !== null && <span>{item.marketable ? "Marketable" : "Not marketable"}</span>}
          {item.folder && <span>Folder: {item.folder}</span>}
        </div>

        {item.tags.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-1">
            {item.tags.map((tag) => (
              <span
                key={tag.id}
                className="rounded px-1.5 py-0.5 text-xs"
                style={{ backgroundColor: tag.color, color: "#111" }}
              >
                {tag.name}
              </span>
            ))}
          </div>
        )}
        {item.note && <p className="mt-2 text-sm text-fg-muted italic">{item.note}</p>}

        {candidateUrl ? (
          resolvedUrl ? (
            <PriceChart url={resolvedUrl} />
          ) : (
            <p className="mt-5 text-sm text-fg-subtle">
              {analyze.isError ? "Could not load price data for this item." : "Loading price data…"}
            </p>
          )
        ) : (
          <p className="mt-5 text-sm text-fg-subtle">Price data unavailable for this item.</p>
        )}

        {candidateUrl && (
          <button
            type="button"
            onClick={openInMarketAnalyzer}
            className="mt-4 w-full rounded bg-quality-unique px-3 py-2 text-sm font-medium text-black hover:opacity-90"
          >
            View in Market Analyzer
          </button>
        )}
      </div>
    </div>
  );
}
