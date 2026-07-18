import { useState } from "react";
import { useInventory } from "../backpack/api";
import type { BackpackItem } from "../backpack/api";
import { QUALITY_NAMES, qualityColor, qualityName } from "../backpack/quality";
import { EMPTY_FILTERS, hasAnyFilter, useSearchItems, useSimulateTrade } from "./api";
import type { ItemKeyInput, ItemSearchResult, SearchFilters, SimulatedTradeView } from "./api";

const GIVEN_MIME = "application/x-tf2-terminal-given";
const RECEIVED_MIME = "application/x-tf2-terminal-received";
const STARS_MAX = 5;
const QUALITY_OPTIONS = Object.entries(QUALITY_NAMES).map(([id, name]) => ({ id: Number(id), name }));

interface GivenEntry {
  assetId: string;
  name: string;
  quality: number;
}

interface ReceivedEntry {
  key: ItemKeyInput;
  name: string;
  quality: number;
}

function itemToKey(item: ItemSearchResult): ItemKeyInput {
  return {
    defindex: item.defindex,
    quality: item.quality,
    effect_id: item.effect_id,
    killstreak_tier: item.killstreak_tier,
    australium: item.australium,
    festivized: item.festivized,
    craftable: item.craftable,
  };
}

function stars(count: number): string {
  return "★".repeat(count) + "☆".repeat(Math.max(0, STARS_MAX - count));
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

export function Simulator() {
  const { data: inventory = [] } = useInventory();
  const [ownedFilter, setOwnedFilter] = useState("");
  const [given, setGiven] = useState<GivenEntry[]>([]);
  const [received, setReceived] = useState<ReceivedEntry[]>([]);
  const [filters, setFilters] = useState<SearchFilters>(EMPTY_FILTERS);

  const search = useSearchItems(filters);
  const verdict = useSimulateTrade(
    given.map((g) => g.assetId),
    received.map((r) => r.key),
  );

  function addGiven(item: BackpackItem) {
    setGiven((prev) =>
      prev.some((g) => g.assetId === item.asset_id)
        ? prev
        : [...prev, { assetId: item.asset_id, name: item.name, quality: item.quality }],
    );
  }
  function removeGiven(assetId: string) {
    setGiven((prev) => prev.filter((g) => g.assetId !== assetId));
  }
  function addReceived(item: ItemSearchResult) {
    const key = itemToKey(item);
    setReceived((prev) =>
      prev.some((r) => JSON.stringify(r.key) === JSON.stringify(key))
        ? prev
        : [...prev, { key, name: item.name, quality: item.quality }],
    );
  }
  function removeReceived(index: number) {
    setReceived((prev) => prev.filter((_, i) => i !== index));
  }

  const filteredInventory = inventory.filter((i) => i.name.toLowerCase().includes(ownedFilter.toLowerCase()));

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto bg-charcoal p-4 text-fg">
      <h2 className="mb-4 text-lg font-semibold">Simulator</h2>
      <p className="mb-4 text-xs text-fg-subtle">
        Drag items (or click "Add") into either bucket to see how a hypothetical trade would rate.
      </p>

      <div className="grid flex-1 grid-cols-1 gap-4 lg:grid-cols-3">
        <YourItemsPane items={filteredInventory} filter={ownedFilter} onFilterChange={setOwnedFilter} onAdd={addGiven} />

        <div className="flex flex-col gap-4">
          <DropZone
            title={`You Give (${given.length})`}
            mime={GIVEN_MIME}
            onDropItem={(data) => addGiven(JSON.parse(data) as BackpackItem)}
          >
            {given.map((g) => (
              <Chip key={g.assetId} label={g.name} color={qualityColor(g.quality)} onRemove={() => removeGiven(g.assetId)} />
            ))}
          </DropZone>

          <DropZone
            title={`You Receive (${received.length})`}
            mime={RECEIVED_MIME}
            onDropItem={(data) => addReceived(JSON.parse(data) as ItemSearchResult)}
          >
            {received.map((r, index) => (
              <Chip key={index} label={r.name} color={qualityColor(r.quality)} onRemove={() => removeReceived(index)} />
            ))}
          </DropZone>

          {verdict.isError && <p className="text-sm text-red-400">{verdict.error.message}</p>}
          {verdict.isLoading && <p className="text-sm text-fg-subtle">Valuing…</p>}
          {verdict.data && <VerdictPanel verdict={verdict.data} />}
          {!verdict.data && !verdict.isError && !verdict.isLoading && given.length === 0 && received.length === 0 && (
            <p className="text-sm text-fg-subtle">Add items to both sides to get a verdict.</p>
          )}
        </div>

        <SearchPane filters={filters} onFiltersChange={setFilters} results={search.data ?? []} isLoading={search.isLoading} onAdd={addReceived} />
      </div>
    </div>
  );
}

function YourItemsPane({
  items,
  filter,
  onFilterChange,
  onAdd,
}: {
  items: BackpackItem[];
  filter: string;
  onFilterChange: (value: string) => void;
  onAdd: (item: BackpackItem) => void;
}) {
  return (
    <div className="flex min-h-0 flex-col rounded border border-charcoal-border">
      <div className="border-b border-charcoal-border bg-charcoal-raised px-3 py-2 text-sm font-medium">Your Items</div>
      <input
        type="text"
        value={filter}
        onChange={(e) => onFilterChange(e.target.value)}
        placeholder="Filter your backpack…"
        className="border-b border-charcoal-border bg-charcoal px-3 py-2 text-sm placeholder:text-fg-subtle focus:outline-none"
      />
      <div className="max-h-96 flex-1 overflow-y-auto">
        {items.length === 0 ? (
          <p className="px-3 py-3 text-sm text-fg-subtle">No matching items.</p>
        ) : (
          items.slice(0, 200).map((item) => (
            <div
              key={item.asset_id}
              draggable
              onDragStart={(e) => e.dataTransfer.setData(GIVEN_MIME, JSON.stringify(item))}
              className="flex items-center justify-between border-b border-charcoal-border px-3 py-1.5 text-sm last:border-0 hover:bg-charcoal-raised"
            >
              <span className="truncate" style={{ color: qualityColor(item.quality) }}>
                {item.name}
              </span>
              <button
                type="button"
                onClick={() => onAdd(item)}
                className="ml-2 shrink-0 rounded bg-charcoal-raised px-2 py-0.5 text-xs hover:bg-charcoal-border"
              >
                Add
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function DropZone({
  title,
  mime,
  onDropItem,
  children,
}: {
  title: string;
  mime: string;
  onDropItem: (data: string) => void;
  children: React.ReactNode;
}) {
  const [isOver, setIsOver] = useState(false);
  return (
    <div
      onDragOver={(e) => {
        e.preventDefault();
        setIsOver(true);
      }}
      onDragLeave={() => setIsOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setIsOver(false);
        const data = e.dataTransfer.getData(mime);
        if (data) onDropItem(data);
      }}
      className={`min-h-24 rounded border-2 border-dashed p-2 ${isOver ? "border-quality-unique bg-charcoal-raised" : "border-charcoal-border"}`}
    >
      <div className="mb-2 text-sm font-medium text-fg-muted">{title}</div>
      <div className="flex flex-wrap gap-1.5">{children}</div>
    </div>
  );
}

function Chip({ label, color, onRemove }: { label: string; color: string; onRemove: () => void }) {
  return (
    <span className="flex items-center gap-1 rounded bg-charcoal px-2 py-1 text-xs" style={{ color }}>
      {label}
      <button type="button" onClick={onRemove} className="text-fg-subtle hover:text-fg-muted">
        ×
      </button>
    </span>
  );
}

function SearchPane({
  filters,
  onFiltersChange,
  results,
  isLoading,
  onAdd,
}: {
  filters: SearchFilters;
  onFiltersChange: (filters: SearchFilters) => void;
  results: ItemSearchResult[];
  isLoading: boolean;
  onAdd: (item: ItemSearchResult) => void;
}) {
  const filtersActive = hasAnyFilter(filters);

  return (
    <div className="flex min-h-0 flex-col rounded border border-charcoal-border">
      <div className="border-b border-charcoal-border bg-charcoal-raised px-3 py-2 text-sm font-medium">Search</div>
      <div className="flex flex-col gap-2 border-b border-charcoal-border p-3">
        <input
          type="text"
          value={filters.name}
          onChange={(e) => onFiltersChange({ ...filters, name: e.target.value })}
          placeholder="Item name…"
          className="rounded border border-charcoal-border bg-charcoal px-2 py-1.5 text-sm placeholder:text-fg-subtle focus:outline-none"
        />
        <div className="grid grid-cols-2 gap-2">
          <select
            value={filters.quality ?? ""}
            onChange={(e) => onFiltersChange({ ...filters, quality: e.target.value === "" ? null : Number(e.target.value) })}
            className="rounded border border-charcoal-border bg-charcoal px-2 py-1.5 text-sm"
          >
            <option value="">Any quality</option>
            {QUALITY_OPTIONS.map((q) => (
              <option key={q.id} value={q.id}>
                {q.name}
              </option>
            ))}
          </select>
          <select
            value={filters.hasEffect === null ? "" : String(filters.hasEffect)}
            onChange={(e) =>
              onFiltersChange({ ...filters, hasEffect: e.target.value === "" ? null : e.target.value === "true" })
            }
            className="rounded border border-charcoal-border bg-charcoal px-2 py-1.5 text-sm"
          >
            <option value="">Any effect</option>
            <option value="true">Unusual only</option>
            <option value="false">No effect</option>
          </select>
        </div>
        <div className="flex gap-3 text-xs text-fg-muted">
          <label className="flex items-center gap-1">
            <input
              type="checkbox"
              checked={filters.australium === true}
              onChange={(e) => onFiltersChange({ ...filters, australium: e.target.checked ? true : null })}
            />
            Australium
          </label>
          <label className="flex items-center gap-1">
            <input
              type="checkbox"
              checked={filters.craftable === false}
              onChange={(e) => onFiltersChange({ ...filters, craftable: e.target.checked ? false : null })}
            />
            Uncraftable
          </label>
        </div>
      </div>
      <div className="max-h-96 flex-1 overflow-y-auto">
        {!filtersActive && (
          <p className="px-3 py-3 text-sm text-fg-subtle">Type a name or pick a filter to search the item catalog.</p>
        )}
        {filtersActive && isLoading && <p className="px-3 py-3 text-sm text-fg-subtle">Searching…</p>}
        {filtersActive && !isLoading && results.length === 0 && (
          <p className="px-3 py-3 text-sm text-fg-subtle">No matches.</p>
        )}
        {results.map((item) => (
          <div
            key={item.item_id}
            draggable
            onDragStart={(e) => e.dataTransfer.setData(RECEIVED_MIME, JSON.stringify(item))}
            className="flex items-center justify-between border-b border-charcoal-border px-3 py-1.5 text-sm last:border-0 hover:bg-charcoal-raised"
          >
            <span className="truncate" style={{ color: qualityColor(item.quality) }}>
              {qualityName(item.quality)} {item.name}
            </span>
            <button
              type="button"
              onClick={() => onAdd(item)}
              className="ml-2 shrink-0 rounded bg-charcoal-raised px-2 py-0.5 text-xs hover:bg-charcoal-border"
            >
              Add
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

function VerdictPanel({ verdict }: { verdict: SimulatedTradeView }) {
  return (
    <div className="rounded border border-charcoal-border bg-charcoal-raised p-3">
      <div className="flex items-center justify-between">
        <span className="text-lg text-quality-unique">{stars(verdict.stars)}</span>
        <span className={`text-xs font-medium uppercase ${riskColor(verdict.risk)}`}>{verdict.risk} risk</span>
      </div>
      <div className="mt-2 grid grid-cols-2 gap-2 text-sm sm:grid-cols-4">
        <div>
          <div className="text-xs text-fg-muted">Net</div>
          <div className="font-semibold">{formatSignedRef(verdict.net_ref)}</div>
        </div>
        <div>
          <div className="text-xs text-fg-muted">ROI</div>
          <div className="font-semibold">{formatPct(verdict.roi_pct)}</div>
        </div>
        <div>
          <div className="text-xs text-fg-muted">You Give</div>
          <div className="font-semibold">{(verdict.given_total_ref ?? 0).toFixed(2)} ref</div>
        </div>
        <div>
          <div className="text-xs text-fg-muted">You Receive</div>
          <div className="font-semibold">{(verdict.received_total_ref ?? 0).toFixed(2)} ref</div>
        </div>
      </div>
      {verdict.explanation.length > 0 && (
        <ul className="mt-2 list-disc space-y-1 pl-5 text-xs text-fg-muted">
          {verdict.explanation.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      )}
      {verdict.counteroffer_additional_ref !== null && (
        <p className="mt-2 rounded border border-amber-900 bg-amber-950/30 px-2 py-1 text-xs text-amber-300">
          Consider asking for ~{verdict.counteroffer_additional_ref.toFixed(2)} more ref to break even.
        </p>
      )}
    </div>
  );
}
