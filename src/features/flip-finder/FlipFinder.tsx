import { useState } from "react";
import {
  useAddToWatchlist,
  useFlipOpportunities,
  useRemoveFromWatchlist,
  useWatchlist,
} from "./api";
import type { FlipOpportunityView } from "./api";

// Specta exports Rust's `f64` as `number | null` even for fields that are
// effectively always present at runtime (see the same note in
// MarketAnalyzer.tsx) — null-coalesce for display.
function formatRef(value: number | null): string {
  if (value === null) return "—";
  return `${value.toFixed(2)} ref`;
}

function formatSignedRef(value: number | null): string {
  if (value === null) return "—";
  const sign = value > 0 ? "+" : "";
  return `${sign}${value.toFixed(2)} ref`;
}

function formatPct(value: number | null): string {
  if (value === null) return "—";
  const sign = value > 0 ? "+" : "";
  return `${sign}${value.toFixed(1)}%`;
}

function formatHours(value: number | null): string {
  if (value === null) return "—";
  if (value < 1) return "<1h";
  if (value < 48) return `${value.toFixed(0)}h`;
  return `${(value / 24).toFixed(1)}d`;
}

export function FlipFinder() {
  const [minRoiInput, setMinRoiInput] = useState("");
  const [minConfidenceInput, setMinConfidenceInput] = useState("");
  const [watchUrl, setWatchUrl] = useState("");

  const minRoiPct = minRoiInput.trim() === "" ? null : Number(minRoiInput);
  const minConfidence = minConfidenceInput.trim() === "" ? null : Number(minConfidenceInput);

  const opportunities = useFlipOpportunities(minRoiPct, minConfidence);
  const watchlist = useWatchlist();
  const addToWatchlist = useAddToWatchlist();
  const removeFromWatchlist = useRemoveFromWatchlist();

  function handleAddToWatchlist(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = watchUrl.trim();
    if (!trimmed) return;
    addToWatchlist.mutate(trimmed, { onSuccess: () => setWatchUrl("") });
  }

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto bg-charcoal p-4 text-zinc-200">
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">Flip Finder</h2>
        <span className="text-xs text-zinc-500">{opportunities.isFetching ? "Scanning…" : "Refreshes automatically"}</span>
      </div>

      <div className="mb-4 flex flex-wrap items-end gap-3 rounded border border-charcoal-border bg-charcoal-raised p-3">
        <div className="flex flex-col gap-1">
          <label className="text-xs text-zinc-400">Min ROI %</label>
          <input
            type="number"
            step="0.1"
            value={minRoiInput}
            onChange={(e) => setMinRoiInput(e.target.value)}
            placeholder="any"
            className="w-24 rounded border border-charcoal-border bg-charcoal px-3 py-2 text-sm placeholder:text-zinc-500"
          />
        </div>
        <div className="flex flex-col gap-1">
          <label className="text-xs text-zinc-400">Min Confidence</label>
          <input
            type="number"
            step="1"
            value={minConfidenceInput}
            onChange={(e) => setMinConfidenceInput(e.target.value)}
            placeholder="any"
            className="w-24 rounded border border-charcoal-border bg-charcoal px-3 py-2 text-sm placeholder:text-zinc-500"
          />
        </div>
      </div>

      {opportunities.isError && (
        <p className="mb-4 rounded border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-400">
          {opportunities.error.message}
        </p>
      )}
      {opportunities.isLoading && !opportunities.isError && <p className="text-sm text-zinc-500">Scanning for flip opportunities…</p>}
      {!opportunities.isLoading && !opportunities.isError && (opportunities.data ?? []).length === 0 && (
        <p className="mb-6 text-sm text-zinc-500">
          No flip opportunities match right now. Try loosening the filters, or watch more items below.
        </p>
      )}

      {(opportunities.data ?? []).length > 0 && (
        <div className="mb-6 overflow-x-auto rounded border border-charcoal-border">
          <table className="w-full min-w-[720px] text-sm">
            <thead>
              <tr className="border-b border-charcoal-border bg-charcoal-raised text-left text-xs text-zinc-400">
                <th className="px-3 py-2 font-medium">Item</th>
                <th className="px-3 py-2 font-medium text-right">Buy</th>
                <th className="px-3 py-2 font-medium text-right">Sell</th>
                <th className="px-3 py-2 font-medium text-right">Profit</th>
                <th className="px-3 py-2 font-medium text-right">ROI</th>
                <th className="px-3 py-2 font-medium text-right">Confidence</th>
                <th className="px-3 py-2 font-medium text-right">Est. Sale Time</th>
              </tr>
            </thead>
            <tbody>
              {(opportunities.data ?? []).map((opportunity, index) => (
                <OpportunityRow key={`${opportunity.item_name}-${index}`} opportunity={opportunity} />
              ))}
            </tbody>
          </table>
        </div>
      )}

      <div>
        <h3 className="mb-2 text-sm font-medium text-zinc-400">Watchlist</h3>
        <form onSubmit={handleAddToWatchlist} className="mb-3 flex gap-2">
          <input
            type="text"
            value={watchUrl}
            onChange={(e) => setWatchUrl(e.target.value)}
            placeholder="Paste a backpack.tf classifieds URL…"
            className="flex-1 rounded border border-charcoal-border bg-charcoal-raised px-3 py-2 text-sm placeholder:text-zinc-500 focus:outline-none"
          />
          <button
            type="submit"
            disabled={addToWatchlist.isPending || !watchUrl.trim()}
            className="rounded bg-quality-unique px-4 py-2 text-sm font-medium text-black hover:opacity-90 disabled:opacity-50"
          >
            {addToWatchlist.isPending ? "Adding…" : "Watch"}
          </button>
        </form>
        {addToWatchlist.isError && <p className="mb-3 text-sm text-red-400">{addToWatchlist.error.message}</p>}

        <div className="rounded border border-charcoal-border">
          {(watchlist.data ?? []).length === 0 ? (
            <p className="px-3 py-3 text-sm text-zinc-500">Not watching any items yet.</p>
          ) : (
            watchlist.data!.map((item) => (
              <div
                key={item.item_id}
                className="flex items-center justify-between border-b border-charcoal-border px-3 py-2 text-sm last:border-0"
              >
                <span>{item.item_name}</span>
                <button
                  type="button"
                  onClick={() => removeFromWatchlist.mutate(item.item_id ?? 0)}
                  className="rounded bg-charcoal-raised px-2 py-1 text-xs text-red-400 hover:bg-charcoal-border"
                >
                  Remove
                </button>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}

function OpportunityRow({ opportunity }: { opportunity: FlipOpportunityView }) {
  return (
    <tr className="border-b border-charcoal-border last:border-0">
      <td className="px-3 py-2">
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="font-medium">{opportunity.item_name}</span>
          {opportunity.is_watched && <Badge label="Watched" colorClass="bg-sky-950/60 text-sky-300" />}
          {opportunity.is_high_volume && <Badge label="High Volume" colorClass="bg-emerald-950/60 text-emerald-300" />}
          {opportunity.is_mover && <Badge label="Mover" colorClass="bg-amber-950/60 text-amber-300" />}
        </div>
      </td>
      <td className="px-3 py-2 text-right">{formatRef(opportunity.buy_price_ref)}</td>
      <td className="px-3 py-2 text-right">
        {formatRef(opportunity.sell_price_ref)}
        {opportunity.quicksell_ref !== null && (
          <div className="text-xs text-zinc-500">quicksell {formatRef(opportunity.quicksell_ref)}</div>
        )}
      </td>
      <td className="px-3 py-2 text-right font-medium text-emerald-400">{formatSignedRef(opportunity.expected_profit_ref)}</td>
      <td className="px-3 py-2 text-right">{formatPct(opportunity.roi_pct)}</td>
      <td className="px-3 py-2 text-right">{(opportunity.confidence ?? 0).toFixed(0)}</td>
      <td className="px-3 py-2 text-right text-zinc-400">{formatHours(opportunity.est_sale_time_hours)}</td>
    </tr>
  );
}

function Badge({ label, colorClass }: { label: string; colorClass: string }) {
  return <span className={`rounded px-1.5 py-0.5 text-[10px] font-medium uppercase ${colorClass}`}>{label}</span>;
}
