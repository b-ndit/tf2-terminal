import { useActiveTrades } from "./api";
import type { AnalyzedTradeOffer } from "./api";

const STARS_MAX = 5;

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

function riskColor(risk: string): string {
  switch (risk) {
    case "low":
      return "text-emerald-400";
    case "medium":
      return "text-amber-400";
    default:
      return "text-red-400";
  }
}

function stars(count: number): string {
  return "★".repeat(count) + "☆".repeat(Math.max(0, STARS_MAX - count));
}

export function TradeAnalyzer() {
  const { data: trades = [], isLoading, isFetching, error } = useActiveTrades();

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto bg-charcoal p-4 text-fg">
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">Trade Analyzer</h2>
        <span className="text-xs text-fg-subtle">{isFetching ? "Checking for offers…" : "Refreshes automatically"}</span>
      </div>

      {error && (
        <p className="mb-4 rounded border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-400">{error.message}</p>
      )}

      {isLoading && !error && <p className="text-sm text-fg-subtle">Loading active trade offers…</p>}

      {!isLoading && !error && trades.length === 0 && (
        <p className="text-sm text-fg-subtle">
          No active trade offers right now. This panel checks for new offers automatically every 20 seconds.
        </p>
      )}

      <div className="flex flex-col gap-4">
        {trades.map((trade) => (
          <TradeCard key={trade.trade_offer_id} trade={trade} />
        ))}
      </div>
    </div>
  );
}

function TradeCard({ trade }: { trade: AnalyzedTradeOffer }) {
  return (
    <div className="rounded border border-charcoal-border bg-charcoal-raised p-4" data-testid="trade-card">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <div className="text-sm font-medium">Partner: {trade.partner_steam_id}</div>
          {trade.message && <div className="text-xs italic text-fg-muted">"{trade.message}"</div>}
        </div>
        <div className="flex items-center gap-3">
          <span className="text-lg text-quality-unique" title={`${trade.stars}/5 stars`}>
            {stars(trade.stars)}
          </span>
          <span className={`text-xs font-medium uppercase ${riskColor(trade.risk)}`}>{trade.risk} risk</span>
        </div>
      </div>

      <div className="mt-3 grid grid-cols-1 gap-4 sm:grid-cols-2">
        <ItemList title={`You Give (${trade.given_items.length})`} items={trade.given_items} />
        <ItemList title={`You Receive (${trade.received_items.length})`} items={trade.received_items} />
      </div>

      <div className="mt-3 grid grid-cols-2 gap-3 sm:grid-cols-4">
        <Stat label="Net" value={formatSignedRef(trade.net_ref)} />
        <Stat label="ROI" value={formatPct(trade.roi_pct)} />
        <Stat label="You Give" value={formatRef(trade.given_total_ref)} />
        <Stat label="You Receive" value={formatRef(trade.received_total_ref)} />
      </div>

      {trade.explanation.length > 0 && (
        <ul className="mt-3 list-disc space-y-1 pl-5 text-xs text-fg-muted">
          {trade.explanation.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      )}

      {trade.counteroffer_additional_ref !== null && <CounterofferSuggestion trade={trade} />}
    </div>
  );
}

function ItemList({ title, items }: { title: string; items: AnalyzedTradeOffer["given_items"] }) {
  return (
    <div className="rounded border border-charcoal-border">
      <div className="border-b border-charcoal-border px-3 py-1.5 text-xs font-medium text-fg-muted">{title}</div>
      {items.length === 0 ? (
        <p className="px-3 py-2 text-xs text-fg-subtle">No items</p>
      ) : (
        <ul className="divide-y divide-charcoal-border">
          {items.map((item, index) => (
            <li key={`${item.name}-${index}`} className="flex items-center justify-between px-3 py-1.5 text-sm">
              <span className={item.estimated_ref === null ? "italic text-fg-subtle" : "text-fg"}>{item.name}</span>
              <span className="text-xs text-fg-muted">{formatRef(item.estimated_ref)}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-charcoal-border bg-charcoal px-3 py-2">
      <div className="text-xs text-fg-muted">{label}</div>
      <div className="text-sm font-semibold">{value}</div>
    </div>
  );
}

// No Steam write-calls: this only formats a message for the user to send
// themselves (docs/DESIGN.md §2 — analysis only, no trade automation).
function CounterofferSuggestion({ trade }: { trade: AnalyzedTradeOffer }) {
  const additionalRef = trade.counteroffer_additional_ref ?? 0;
  const keysBreakdown =
    trade.counteroffer_additional_keys !== null && trade.counteroffer_additional_metal_ref !== null
      ? ` (~${trade.counteroffer_additional_keys.toFixed(0)} keys, ${trade.counteroffer_additional_metal_ref.toFixed(2)} ref)`
      : "";
  const message = `Hey, based on current market prices this trade looks about ${additionalRef.toFixed(2)} ref light for me — could you add ~${additionalRef.toFixed(2)} ref${keysBreakdown} to balance it out?`;

  return (
    <div className="mt-3 rounded border border-amber-900 bg-amber-950/30 px-3 py-2">
      <p className="text-xs text-amber-300">
        Suggested counteroffer: ask for an additional ~{additionalRef.toFixed(2)} ref{keysBreakdown}
      </p>
      <button
        type="button"
        onClick={() => navigator.clipboard.writeText(message)}
        className="mt-2 rounded bg-charcoal-raised px-2 py-1 text-xs hover:bg-charcoal-border"
      >
        Copy message
      </button>
    </div>
  );
}
