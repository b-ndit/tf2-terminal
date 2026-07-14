import { useMemo } from "react";
import type { BackpackItem } from "./api";

interface StatsBarProps {
  items: BackpackItem[];
}

export function StatsBar({ items }: StatsBarProps) {
  const stats = useMemo(() => {
    let unusual = 0;
    let strange = 0;
    let australium = 0;
    let favorites = 0;
    for (const item of items) {
      if (item.effect_id !== null) unusual++;
      if (item.quality === 11) strange++;
      if (item.australium) australium++;
      if (item.meta.favorite) favorites++;
    }
    return { unusual, strange, australium, favorites };
  }, [items]);

  return (
    <div className="flex items-center gap-4 border-b border-charcoal-border bg-charcoal px-4 py-2 text-sm text-zinc-300">
      <span data-testid="stats-total" className="font-semibold text-zinc-100">
        Σ {items.length} items
      </span>
      {stats.unusual > 0 && <Stat label="Unusual" value={stats.unusual} color="var(--color-quality-unusual)" />}
      {stats.strange > 0 && <Stat label="Strange" value={stats.strange} color="var(--color-quality-strange)" />}
      {stats.australium > 0 && <Stat label="Australium" value={stats.australium} color="#e7b53b" />}
      {stats.favorites > 0 && <Stat label="Favorites" value={stats.favorites} color="var(--color-quality-unique)" />}
    </div>
  );
}

function Stat({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <span style={{ color }}>
      {label}: {value}
    </span>
  );
}
