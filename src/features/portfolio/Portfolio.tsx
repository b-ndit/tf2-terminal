import { useEffect, useRef, useState } from "react";
import { LineSeries, createChart, type UTCTimestamp } from "lightweight-charts";
import { usePlWindows, usePortfolioHistory, usePortfolioSnapshot, useRefreshPortfolio, useWinnersLosers } from "./api";
import type { ItemMoverView, PlWindowsView, PortfolioSnapshotView } from "./api";

function formatRef(value: number | null): string {
  if (value === null) return "—";
  return `${value.toFixed(2)} ref`;
}

function formatKeys(value: number | null): string {
  if (value === null) return "—";
  return `${value.toFixed(2)} keys`;
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

export function Portfolio() {
  const snapshot = usePortfolioSnapshot();
  const history = usePortfolioHistory();
  const plWindows = usePlWindows();
  const refresh = useRefreshPortfolio();
  const [moversWindow, setMoversWindow] = useState(7);
  const movers = useWinnersLosers(moversWindow);

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto bg-charcoal p-4 text-zinc-200">
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">Portfolio</h2>
        <button
          type="button"
          onClick={() => refresh.mutate()}
          disabled={refresh.isPending}
          className="rounded bg-charcoal-raised px-3 py-1 text-sm hover:bg-charcoal-border disabled:opacity-50"
        >
          {refresh.isPending ? "Refreshing…" : "Refresh"}
        </button>
      </div>

      {snapshot.isError && (
        <p className="mb-4 rounded border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-400">
          {snapshot.error.message}
        </p>
      )}
      {refresh.isError && <p className="mb-4 text-sm text-red-400">{refresh.error.message}</p>}
      {snapshot.isLoading && !snapshot.isError && <p className="text-sm text-zinc-500">Valuing your backpack…</p>}

      {snapshot.data && <SnapshotTiles snapshot={snapshot.data} />}
      {plWindows.data && <PlWindowsRow windows={plWindows.data} />}

      <div className="mt-5">
        <div className="mb-1.5 text-sm font-medium text-zinc-300">Performance (90 days)</div>
        {history.isSuccess && history.data.length < 2 && (
          <p className="text-sm text-zinc-500">Not enough snapshots yet to chart a trend — check back tomorrow.</p>
        )}
        {history.isSuccess && history.data.length >= 2 && <PerformanceChart snapshots={history.data} />}
      </div>

      <div className="mt-5">
        <div className="mb-2 flex items-center justify-between">
          <div className="text-sm font-medium text-zinc-300">Winners &amp; Losers</div>
          <div className="flex gap-1">
            {[1, 7, 30].map((days) => (
              <button
                key={days}
                type="button"
                onClick={() => setMoversWindow(days)}
                className={`rounded px-2 py-1 text-xs ${
                  moversWindow === days ? "bg-charcoal-raised text-zinc-100" : "text-zinc-400 hover:text-zinc-200"
                }`}
              >
                {days}D
              </button>
            ))}
          </div>
        </div>
        {movers.data && <WinnersLosers movers={movers.data} />}
      </div>
    </div>
  );
}

function SnapshotTiles({ snapshot }: { snapshot: PortfolioSnapshotView }) {
  return (
    <div className="grid grid-cols-3 gap-3 sm:grid-cols-6">
      <Stat label="Total Value" value={formatRef(snapshot.total_ref)} />
      <Stat label="In Keys" value={formatKeys(snapshot.total_keys)} />
      <Stat label="Pure Keys" value={String(snapshot.pure_keys)} />
      <Stat label="Pure Metal" value={formatRef(snapshot.pure_metal_ref)} />
      <Stat label="Items" value={String(snapshot.item_count)} />
      <Stat label="Unusuals / Australiums" value={`${snapshot.unusual_count} / ${snapshot.australium_count}`} />
    </div>
  );
}

function PlWindowsRow({ windows }: { windows: PlWindowsView }) {
  return (
    <div className="mt-3 grid grid-cols-3 gap-3">
      <PlTile label="1 Day" window={windows.d1} />
      <PlTile label="7 Days" window={windows.d7} />
      <PlTile label="30 Days" window={windows.d30} />
    </div>
  );
}

function PlTile({ label, window }: { label: string; window: PlWindowsView["d1"] }) {
  const color = window === null ? "text-zinc-500" : (window.pct ?? 0) >= 0 ? "text-emerald-400" : "text-red-400";
  return (
    <div className="rounded border border-charcoal-border bg-charcoal-raised px-3 py-2">
      <div className="text-xs text-zinc-400">{label}</div>
      <div className={`text-sm font-semibold ${color}`}>
        {window === null ? "—" : `${formatSignedRef(window.abs_ref)} (${formatPct(window.pct)})`}
      </div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-charcoal-border bg-charcoal-raised px-3 py-2">
      <div className="text-xs text-zinc-400">{label}</div>
      <div className="text-sm font-semibold">{value}</div>
    </div>
  );
}

function PerformanceChart({ snapshots }: { snapshots: PortfolioSnapshotView[] }) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const chart = createChart(container, {
      autoSize: true,
      layout: { background: { color: "#17181c" }, textColor: "#a1a1aa" },
      grid: { vertLines: { color: "#2a2b31" }, horzLines: { color: "#2a2b31" } },
      timeScale: { borderColor: "#2a2b31" },
      rightPriceScale: { borderColor: "#2a2b31" },
    });

    const series = chart.addSeries(LineSeries, { color: "#ffd700", lineWidth: 2 });
    const data = snapshots
      .map((s) => ({ time: (s.ts ?? 0) as UTCTimestamp, value: s.total_ref ?? 0 }))
      .sort((a, b) => a.time - b.time);
    series.setData(data);
    chart.timeScale().fitContent();

    return () => chart.remove();
  }, [snapshots]);

  return <div ref={containerRef} data-testid="portfolio-chart" className="h-64 w-full rounded border border-charcoal-border" />;
}

function WinnersLosers({ movers }: { movers: ItemMoverView[] }) {
  const priced = movers.filter((m) => m.change_pct !== null);
  const winners = priced.slice(0, 5);
  const losers = priced.slice(-5).reverse();

  if (priced.length === 0) {
    return <p className="text-sm text-zinc-500">Not enough price history yet to rank movers.</p>;
  }

  return (
    <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
      <MoverList title="Winners" items={winners} />
      <MoverList title="Losers" items={losers} />
    </div>
  );
}

function MoverList({ title, items }: { title: string; items: ItemMoverView[] }) {
  return (
    <div className="rounded border border-charcoal-border">
      <div className="border-b border-charcoal-border bg-charcoal-raised px-3 py-1.5 text-sm font-medium">{title}</div>
      {items.length === 0 ? (
        <p className="px-3 py-3 text-sm text-zinc-500">Nothing to show.</p>
      ) : (
        <ul className="divide-y divide-charcoal-border">
          {items.map((item, index) => (
            <li key={`${item.item_name}-${index}`} className="flex items-center justify-between px-3 py-1.5 text-sm">
              <span>
                {item.item_name}
                {item.count > 1 && <span className="ml-1 text-xs text-zinc-500">×{item.count}</span>}
              </span>
              <span className={(item.change_pct ?? 0) >= 0 ? "text-emerald-400" : "text-red-400"}>
                {formatPct(item.change_pct)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
