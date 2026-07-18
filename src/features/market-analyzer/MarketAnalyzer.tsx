import { useState } from "react";
import { qualityColor, qualityName } from "../backpack/quality";
import { useAnalyzeClassifiedUrl } from "./api";
import type { ItemAnalytics } from "./api";
import { PriceChart } from "./PriceChart";

// Specta exports Rust's `f64` as `number | null` (accounting for
// NaN/Infinity, which have no JSON representation) even for fields that
// are never actually `Option` on the Rust side. These values are
// effectively always present at runtime; null-coalesce for display.
function formatRef(value: number | null): string {
  if (value === null) return "—";
  return `${value.toFixed(2)} ref`;
}

function formatScore(value: number | null): string {
  return (value ?? 0).toFixed(0);
}

function formatPct(value: number | null): string {
  if (value === null) return "—";
  const sign = value > 0 ? "+" : "";
  return `${sign}${value.toFixed(1)}%`;
}

export function MarketAnalyzer() {
  const [url, setUrl] = useState("");
  const [analyzedUrl, setAnalyzedUrl] = useState<string | null>(null);
  const analyze = useAnalyzeClassifiedUrl();

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = url.trim();
    if (trimmed) {
      analyze.mutate(trimmed, { onSuccess: () => setAnalyzedUrl(trimmed) });
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto bg-charcoal p-4 text-fg">
      <form onSubmit={handleSubmit} className="mb-4 flex gap-2">
        <input
          type="text"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="Paste a backpack.tf classifieds URL…"
          className="flex-1 rounded border border-charcoal-border bg-charcoal-raised px-3 py-2 text-sm placeholder:text-fg-subtle focus:outline-none"
        />
        <button
          type="submit"
          disabled={analyze.isPending || !url.trim()}
          className="rounded bg-quality-unique px-4 py-2 text-sm font-medium text-black hover:opacity-90 disabled:opacity-50"
        >
          {analyze.isPending ? "Analyzing…" : "Analyze"}
        </button>
      </form>

      {analyze.isError && (
        <p className="mb-4 rounded border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-400">
          {analyze.error.message}
        </p>
      )}

      {analyze.data && <AnalyticsResult analytics={analyze.data} url={analyzedUrl} />}

      {!analyze.data && !analyze.isError && (
        <p className="text-sm text-fg-subtle">
          Paste a classifieds search URL (e.g. from backpack.tf) to see live spread, liquidity, demand, and
          buyers/sellers for that item.
        </p>
      )}
    </div>
  );
}

function AnalyticsResult({ analytics, url }: { analytics: ItemAnalytics; url: string | null }) {
  const color = qualityColor(analytics.quality);
  const sortedBuys = [...analytics.buy_listings].sort((a, b) => (b.price_ref ?? 0) - (a.price_ref ?? 0));
  const sortedSells = [...analytics.sell_listings].sort((a, b) => (a.price_ref ?? 0) - (b.price_ref ?? 0));
  const hasTrend = analytics.trend_ma7_ref !== null || analytics.trend_d1_pct !== null;

  return (
    <div data-testid="analytics-result">
      <h2 className="text-lg font-semibold" style={{ color }}>
        {qualityName(analytics.quality)} {analytics.item_name}
        {analytics.effect_id !== null && <span className="ml-2 text-sm text-fg-muted">(Effect #{analytics.effect_id})</span>}
      </h2>

      <div className="mt-3 grid grid-cols-3 gap-3 sm:grid-cols-6">
        <Stat label="Spread" value={analytics.spread_pct !== null ? `${analytics.spread_pct.toFixed(1)}%` : "—"} />
        <Stat label="Spread (ref)" value={formatRef(analytics.spread_abs_ref)} />
        <Stat label="Liquidity" value={formatScore(analytics.liquidity_score)} />
        <Stat label="Demand" value={formatScore(analytics.demand_score)} />
        <Stat label="Est. Sale" value={formatRef(analytics.estimated_sale_price_ref)} />
        <Stat label="Quicksell" value={formatRef(analytics.estimated_quicksell_ref)} />
      </div>

      {hasTrend && (
        <div className="mt-3 grid grid-cols-3 gap-3 sm:grid-cols-6">
          <Stat label="MA7" value={formatRef(analytics.trend_ma7_ref)} />
          <Stat label="MA30" value={formatRef(analytics.trend_ma30_ref)} />
          <Stat label="Volatility" value={analytics.trend_volatility_pct !== null ? `${analytics.trend_volatility_pct.toFixed(1)}%` : "—"} />
          <Stat label="1D" value={formatPct(analytics.trend_d1_pct)} />
          <Stat label="7D" value={formatPct(analytics.trend_d7_pct)} />
          <Stat label="30D" value={formatPct(analytics.trend_d30_pct)} />
        </div>
      )}

      <PriceChart url={url} />

      <div className="mt-5 grid grid-cols-1 gap-4 md:grid-cols-2">
        <ListingTable title={`Buyers (${sortedBuys.length})`} rows={sortedBuys} />
        <ListingTable title={`Sellers (${sortedSells.length})`} rows={sortedSells} />
      </div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-charcoal-border bg-charcoal-raised px-3 py-2">
      <div className="text-xs text-fg-muted">{label}</div>
      <div className="text-sm font-semibold">{value}</div>
    </div>
  );
}

function ListingTable({ title, rows }: { title: string; rows: ItemAnalytics["buy_listings"] }) {
  return (
    <div className="rounded border border-charcoal-border">
      <div className="border-b border-charcoal-border bg-charcoal-raised px-3 py-1.5 text-sm font-medium">{title}</div>
      {rows.length === 0 ? (
        <p className="px-3 py-3 text-sm text-fg-subtle">No active listings observed yet.</p>
      ) : (
        <table className="w-full text-sm">
          <tbody>
            {rows.map((row) => (
              <tr key={row.listing_id} className="border-b border-charcoal-border last:border-0">
                <td className="px-3 py-1.5 text-fg-muted">{row.steam_name ?? row.steam_id}</td>
                <td className="px-3 py-1.5 text-right font-medium">{(row.price_ref ?? 0).toFixed(2)} ref</td>
                <td className="px-3 py-1.5 text-right text-xs text-fg-subtle">{(row.age_hours ?? 0).toFixed(1)}h ago</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
