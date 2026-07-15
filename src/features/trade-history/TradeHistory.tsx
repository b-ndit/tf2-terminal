import { useState } from "react";
import { useSetTradeNotes, useSetTradeRating, useSyncCompletedTrades, useTrades } from "./api";
import type { TradeLedgerView } from "./api";

const STARS_MAX = 5;

function formatSignedRef(value: number | null): string {
  if (value === null) return "—";
  const sign = value > 0 ? "+" : "";
  return `${sign}${value.toFixed(2)} ref`;
}

function formatDate(ts: number | null): string {
  if (ts === null) return "—";
  return new Date(ts * 1000).toLocaleDateString();
}

export function TradeHistory() {
  const trades = useTrades();
  const sync = useSyncCompletedTrades();

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto bg-charcoal p-4 text-zinc-200">
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">Trade History</h2>
        <button
          type="button"
          onClick={() => sync.mutate()}
          disabled={sync.isPending}
          className="rounded bg-quality-unique px-4 py-2 text-sm font-medium text-black hover:opacity-90 disabled:opacity-50"
        >
          {sync.isPending ? "Syncing…" : "Sync"}
        </button>
      </div>

      {sync.isError && (
        <p className="mb-4 rounded border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-400">
          {sync.error.message}
        </p>
      )}
      {sync.isSuccess && (
        <p className="mb-4 text-sm text-zinc-400">
          Checked {sync.data.checked} completed offer(s), imported {sync.data.imported} new trade(s).
        </p>
      )}

      {trades.isError && (
        <p className="mb-4 rounded border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-400">
          {trades.error.message}
        </p>
      )}
      {trades.isLoading && !trades.isError && <p className="text-sm text-zinc-500">Loading trade history…</p>}
      {!trades.isLoading && !trades.isError && (trades.data ?? []).length === 0 && (
        <p className="text-sm text-zinc-500">
          No completed trades recorded yet. Click "Sync" to check for newly-completed Steam trade offers.
        </p>
      )}

      <div className="flex flex-col gap-3">
        {(trades.data ?? []).map((trade) => (
          <TradeRow key={trade.trade_offer_id} trade={trade} />
        ))}
      </div>
    </div>
  );
}

function TradeRow({ trade }: { trade: TradeLedgerView }) {
  const setRating = useSetTradeRating();
  const setNotes = useSetTradeNotes();
  const [notes, setNotesLocal] = useState(trade.notes ?? "");

  return (
    <div className="rounded border border-charcoal-border bg-charcoal-raised p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <div className="text-sm font-medium">Partner: {trade.partner_steam_id}</div>
          <div className="text-xs text-zinc-500">{formatDate(trade.completed_ts)}</div>
        </div>
        <div className="flex items-center gap-3">
          <span className={`text-sm font-semibold ${(trade.net_value_ref ?? 0) >= 0 ? "text-emerald-400" : "text-red-400"}`}>
            {formatSignedRef(trade.net_value_ref)}
          </span>
          <StarRating
            value={trade.rating ?? 0}
            onChange={(rating) => setRating.mutate({ tradeOfferId: trade.trade_offer_id, rating })}
          />
        </div>
      </div>

      <div className="mt-2 grid grid-cols-1 gap-3 sm:grid-cols-2">
        <ItemList title={`Gave (${trade.given.length})`} items={trade.given} />
        <ItemList title={`Received (${trade.received.length})`} items={trade.received} />
      </div>

      <textarea
        value={notes}
        onChange={(e) => setNotesLocal(e.target.value)}
        onBlur={() => {
          if (notes !== (trade.notes ?? "")) {
            setNotes.mutate({ tradeOfferId: trade.trade_offer_id, notes: notes.trim() === "" ? null : notes });
          }
        }}
        placeholder="Add a note…"
        rows={2}
        className="mt-2 w-full rounded border border-charcoal-border bg-charcoal px-2 py-1 text-sm placeholder:text-zinc-500 focus:outline-none"
      />
    </div>
  );
}

function StarRating({ value, onChange }: { value: number; onChange: (rating: number | null) => void }) {
  return (
    <div className="flex items-center gap-0.5 text-lg text-quality-unique">
      {Array.from({ length: STARS_MAX }, (_, i) => i + 1).map((star) => (
        <button
          key={star}
          type="button"
          onClick={() => onChange(star === value ? null : star)}
          className="leading-none"
          title={`Rate ${star}/5`}
        >
          {star <= value ? "★" : "☆"}
        </button>
      ))}
    </div>
  );
}

function ItemList({ title, items }: { title: string; items: TradeLedgerView["given"] }) {
  return (
    <div className="rounded border border-charcoal-border">
      <div className="border-b border-charcoal-border px-2 py-1 text-xs font-medium text-zinc-400">{title}</div>
      {items.length === 0 ? (
        <p className="px-2 py-1.5 text-xs text-zinc-500">No items</p>
      ) : (
        <ul className="divide-y divide-charcoal-border">
          {items.map((item, index) => (
            <li key={`${item.name}-${index}`} className="flex items-center justify-between px-2 py-1 text-xs">
              <span className={item.value_ref === null ? "italic text-zinc-500" : "text-zinc-200"}>{item.name}</span>
              <span className="text-zinc-400">{item.value_ref === null ? "—" : `${item.value_ref.toFixed(2)} ref`}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
