import { useEffect, useState } from "react";
import { useMarketAnalyzerStore } from "../../stores/marketAnalyzerStore";
import { qualityColor, qualityName } from "../backpack/quality";
import { useAnalyzeClassifiedUrl, useKeyRate } from "./api";
import type { ItemAnalytics } from "./api";
import { formatCurrency } from "./currency";
import { PriceChart } from "./PriceChart";

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
  const keyRate = useKeyRate();
  const pendingUrl = useMarketAnalyzerStore((s) => s.pendingUrl);
  const consumePendingUrl = useMarketAnalyzerStore((s) => s.consumePendingUrl);

  function analyzeUrl(target: string) {
    analyze.mutate(target, { onSuccess: () => setAnalyzedUrl(target) });
  }

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = url.trim();
    if (trimmed) {
      analyzeUrl(trimmed);
    }
  }

  // Set when a backpack item tile is clicked (BackpackPanel) — auto-runs
  // the analysis for that item instead of requiring a pasted URL.
  useEffect(() => {
    if (!pendingUrl) return;
    const target = consumePendingUrl();
    if (!target) return;
    setUrl(target);
    analyzeUrl(target);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pendingUrl]);

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

      {analyze.data && <AnalyticsResult analytics={analyze.data} url={analyzedUrl} keyRateRef={keyRate.data ?? null} />}

      {!analyze.data && !analyze.isError && (
        <p className="text-sm text-fg-subtle">
          Paste a classifieds search URL (e.g. from backpack.tf) to see live spread, liquidity, demand, and
          buyers/sellers for that item.
        </p>
      )}
    </div>
  );
}

function AnalyticsResult({
  analytics,
  url,
  keyRateRef,
}: {
  analytics: ItemAnalytics;
  url: string | null;
  keyRateRef: number | null;
}) {
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

      <div
        className="mt-3 grid gap-2"
        style={{ gridTemplateColumns: "repeat(auto-fit, minmax(92px, 1fr))" }}
      >
        <Stat label="Spread" value={analytics.spread_pct !== null ? `${analytics.spread_pct.toFixed(1)}%` : "—"} />
        <Stat label="Spread ref" value={formatCurrency(analytics.spread_abs_ref, keyRateRef)} />
        <Stat label="Liquidity" value={formatScore(analytics.liquidity_score)} />
        <Stat label="Demand" value={formatScore(analytics.demand_score)} />
        <Stat label="Est. Sale" value={formatCurrency(analytics.estimated_sale_price_ref, keyRateRef)} />
        <Stat label="Quicksell" value={formatCurrency(analytics.estimated_quicksell_ref, keyRateRef)} />
        {hasTrend && (
          <>
            <Stat label="MA7" value={formatCurrency(analytics.trend_ma7_ref, keyRateRef)} />
            <Stat label="MA30" value={formatCurrency(analytics.trend_ma30_ref, keyRateRef)} />
            <Stat label="Volatility" value={analytics.trend_volatility_pct !== null ? `${analytics.trend_volatility_pct.toFixed(1)}%` : "—"} />
            <Stat label="1D" value={formatPct(analytics.trend_d1_pct)} />
            <Stat label="7D" value={formatPct(analytics.trend_d7_pct)} />
            <Stat label="30D" value={formatPct(analytics.trend_d30_pct)} />
          </>
        )}
      </div>

      <PriceChart url={url} />

      <div className="mt-5 flex flex-col gap-4">
        <ListingTable title={`Buyers (${sortedBuys.length})`} rows={sortedBuys} keyRateRef={keyRateRef} />
        <ListingTable title={`Sellers (${sortedSells.length})`} rows={sortedSells} keyRateRef={keyRateRef} />
      </div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-charcoal-border bg-charcoal-raised px-2 py-1.5">
      <div className="text-xs text-fg-muted">{label}</div>
      <div className="text-sm font-semibold whitespace-nowrap">{value}</div>
    </div>
  );
}

function ListingTable({
  title,
  rows,
  keyRateRef,
}: {
  title: string;
  rows: ItemAnalytics["buy_listings"];
  keyRateRef: number | null;
}) {
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
                <td className="px-3 py-1.5 text-right font-medium whitespace-nowrap">
                  {formatCurrency(row.price_ref, keyRateRef)}
                </td>
                <td className="px-3 py-1.5 text-right text-xs whitespace-nowrap text-fg-subtle">
                  {(row.age_hours ?? 0).toFixed(1)}h ago
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
