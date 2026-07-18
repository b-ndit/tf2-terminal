import { qualityColor, qualityName } from "../backpack/quality";
import { useLiveFeed } from "./api";
import type { ListingEvent } from "./api";

function describeEvent(event: ListingEvent): { symbol: string; label: string; colorClass: string } {
  if (event.kind === "removed") {
    return { symbol: "▼", label: "delist", colorClass: "text-fg-subtle" };
  }
  if (event.intent === "buy") {
    return event.kind === "new"
      ? { symbol: "●", label: "new buyer", colorClass: "text-emerald-400" }
      : { symbol: "↻", label: "buyer updated", colorClass: "text-emerald-500/70" };
  }
  return event.kind === "new"
    ? { symbol: "▲", label: "new sell", colorClass: "text-sky-400" }
    : { symbol: "↻", label: "sell updated", colorClass: "text-sky-500/70" };
}

export function LiveFeed() {
  const { feed, isLoading, error } = useLiveFeed();

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto bg-charcoal p-4 text-fg">
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">Live Feed</h2>
        <span className="flex items-center gap-1 text-xs text-fg-subtle">
          <span className="h-2 w-2 rounded-full bg-emerald-500" /> live
        </span>
      </div>

      {error && (
        <p className="mb-4 rounded border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-400">{error.message}</p>
      )}
      {isLoading && !error && <p className="text-sm text-fg-subtle">Loading recent listings…</p>}
      {!isLoading && !error && feed.length === 0 && (
        <p className="text-sm text-fg-subtle">No listing activity observed yet.</p>
      )}

      <div className="rounded border border-charcoal-border">
        {feed.map((event, index) => (
          <FeedRow key={`${event.listing_id}-${event.kind}-${index}`} event={event} />
        ))}
      </div>
    </div>
  );
}

function FeedRow({ event }: { event: ListingEvent }) {
  const { symbol, label, colorClass } = describeEvent(event);
  const color = qualityColor(event.quality);

  return (
    <div className="flex items-center justify-between gap-3 border-b border-charcoal-border px-3 py-1.5 text-sm last:border-0">
      <div className="flex min-w-0 items-center gap-2">
        <span className={colorClass}>{symbol}</span>
        <span className="shrink-0 text-xs text-fg-subtle">{label}</span>
        <span className="truncate" style={{ color }}>
          {qualityName(event.quality)} #{event.defindex}
        </span>
        {event.killstreak_tier > 0 && <span className="shrink-0 text-xs text-orange-400">KS{event.killstreak_tier}</span>}
        {event.australium && <span className="shrink-0 text-xs text-amber-500">Australium</span>}
      </div>
      <div className="flex shrink-0 items-center gap-2 text-xs text-fg-muted">
        <span className="max-w-[10rem] truncate">{event.steam_name ?? event.steam_id}</span>
        {event.value_ref !== null && <span className="font-medium text-fg">{event.value_ref.toFixed(2)} ref</span>}
      </div>
    </div>
  );
}
