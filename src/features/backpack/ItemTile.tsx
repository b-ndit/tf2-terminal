import type { CSSProperties, MouseEvent } from "react";
import type { BackpackItem } from "./api";
import { killstreakName, paintToHex, qualityColor, qualityName } from "./quality";
import { TILE_GAP, TILE_SIZE } from "./constants";

interface ItemTileProps {
  item: BackpackItem;
  isSelected: boolean;
  style: CSSProperties;
  onSelect: (assetId: string, additive: boolean) => void;
  onContextMenu: (assetId: string, x: number, y: number) => void;
}

export function ItemTile({ item, isSelected, style, onSelect, onContextMenu }: ItemTileProps) {
  const borderColor = qualityColor(item.quality);
  const hasEffect = item.effect_id !== null;
  const hasKillstreak = item.killstreak_tier > 0;

  function handleClick(e: MouseEvent) {
    onSelect(item.asset_id, e.ctrlKey || e.metaKey);
  }

  function handleContextMenu(e: MouseEvent) {
    e.preventDefault();
    onContextMenu(item.asset_id, e.clientX, e.clientY);
  }

  return (
    <div
      style={{
        ...style,
        padding: TILE_GAP / 2,
      }}
    >
      <div
        data-testid="item-tile"
        onClick={handleClick}
        onContextMenu={handleContextMenu}
        className="group relative flex h-full w-full cursor-pointer flex-col items-center justify-center rounded-md border-2 bg-charcoal-raised px-1 text-center transition-colors"
        style={{
          borderColor,
          boxShadow: isSelected ? `0 0 0 2px ${borderColor}` : undefined,
          width: TILE_SIZE - TILE_GAP,
          height: TILE_SIZE - TILE_GAP,
        }}
      >
        {hasKillstreak && (
          <span
            className="absolute top-1 left-1 text-[10px] font-bold text-quality-unique"
            title={killstreakName(item.killstreak_tier)}
          >
            {"»".repeat(item.killstreak_tier)}
          </span>
        )}

        {hasEffect && (
          <span
            className="absolute top-1 right-1 text-xs text-quality-unusual"
            title="Unusual effect"
          >
            ✦
          </span>
        )}

        {item.paint_id !== null && (
          <span
            className="absolute bottom-1 left-1 h-2.5 w-2.5 rounded-full border border-black/40"
            style={{ backgroundColor: paintToHex(item.paint_id) }}
            title="Painted"
          />
        )}

        {item.meta.pinned && (
          <span
            className="absolute right-1 bottom-1 h-2 w-2 rotate-45 border border-zinc-300 bg-zinc-400"
            title="Pinned"
          />
        )}

        {item.meta.favorite && (
          <span className="absolute top-1 left-1/2 -translate-x-1/2 text-xs text-quality-unique" title="Favorite">
            ★
          </span>
        )}

        <span className="line-clamp-3 text-[11px] leading-tight text-fg">
          {item.meta.custom_label ?? item.name}
        </span>

        <ItemTooltip item={item} />
      </div>
    </div>
  );
}

function ItemTooltip({ item }: { item: BackpackItem }) {
  return (
    <div className="pointer-events-none absolute top-full left-1/2 z-20 mt-1 hidden w-56 -translate-x-1/2 rounded-md border border-charcoal-border bg-charcoal p-2 text-left text-xs text-fg shadow-lg group-hover:block">
      <p className="font-semibold" style={{ color: qualityColor(item.quality) }}>
        {qualityName(item.quality)} {item.name}
      </p>
      {item.meta.custom_label && <p className="text-fg-muted">"{item.meta.custom_label}"</p>}
      {item.killstreak_tier > 0 && <p>{killstreakName(item.killstreak_tier)}</p>}
      {item.strange_count !== null && <p>Strange count: {item.strange_count}</p>}
      {item.craft_number !== null && <p>Craft #{item.craft_number}</p>}
      {item.paint_id !== null && <p>Paint: {paintToHex(item.paint_id)}</p>}
      <p className="text-fg-muted">
        {item.tradable ? "Tradable" : "Not tradable"} ·{" "}
        {item.marketable === true ? "Marketable" : item.marketable === false ? "Not marketable" : "Marketable: unknown"}
      </p>
      {item.meta.folder && <p className="text-fg-muted">Folder: {item.meta.folder}</p>}
      {item.tags.length > 0 && (
        <p className="mt-1 flex flex-wrap gap-1">
          {item.tags.map((tag) => (
            <span
              key={tag.id}
              className="rounded px-1 py-0.5 text-[10px]"
              style={{ backgroundColor: tag.color, color: "#111" }}
            >
              {tag.name}
            </span>
          ))}
        </p>
      )}
      {item.meta.note && <p className="mt-1 text-fg-muted italic">{item.meta.note}</p>}
    </div>
  );
}
