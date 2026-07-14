import { useEffect, useRef } from "react";
import { CandlestickSeries, createChart, type UTCTimestamp } from "lightweight-charts";
import { usePriceHistory } from "./api";

// The container only mounts once real data exists (rather than staying in
// the DOM with `display: none`), so `createChart` always measures a
// correctly-sized, visible element — creating it against a hidden 0×0
// container left the time scale's initial zoom wrong.
function Chart({ bars }: { bars: NonNullable<ReturnType<typeof usePriceHistory>["data"]> }) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const chart = createChart(container, {
      autoSize: true,
      layout: {
        background: { color: "#17181c" },
        textColor: "#a1a1aa",
      },
      grid: {
        vertLines: { color: "#2a2b31" },
        horzLines: { color: "#2a2b31" },
      },
      timeScale: { borderColor: "#2a2b31" },
      rightPriceScale: { borderColor: "#2a2b31" },
    });

    const series = chart.addSeries(CandlestickSeries, {
      upColor: "#26a69a",
      downColor: "#ef5350",
      borderVisible: false,
      wickUpColor: "#26a69a",
      wickDownColor: "#ef5350",
    });

    const data = bars
      .map((bar) => ({
        time: (bar.ts ?? 0) as UTCTimestamp,
        open: bar.open_ref ?? 0,
        high: bar.high_ref ?? 0,
        low: bar.low_ref ?? 0,
        close: bar.close_ref ?? 0,
      }))
      .sort((a, b) => a.time - b.time);
    series.setData(data);
    chart.timeScale().fitContent();

    return () => chart.remove();
  }, [bars]);

  return (
    <div
      ref={containerRef}
      data-testid="price-chart"
      className="h-64 w-full rounded border border-charcoal-border"
    />
  );
}

export function PriceChart({ url }: { url: string | null }) {
  const history = usePriceHistory(url);
  if (url === null) return null;

  return (
    <div className="mt-5">
      <div className="mb-1.5 text-sm font-medium text-zinc-300">Price History (daily)</div>
      {history.isLoading && <p className="text-sm text-zinc-500">Loading price history…</p>}
      {history.isSuccess && history.data.length === 0 && (
        <p className="text-sm text-zinc-500">
          No price history recorded yet for this item — check back after the next snapshot cycle.
        </p>
      )}
      {history.isSuccess && history.data.length > 0 && <Chart bars={history.data} />}
    </div>
  );
}
