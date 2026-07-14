import { useEffect, useMemo, useRef, useState } from "react";
import { Grid, type CellComponentProps } from "react-window";
import type { BackpackItem } from "./api";
import { ItemTile } from "./ItemTile";
import { TILE_SIZE } from "./constants";
import { useBackpackStore } from "../../stores/backpackStore";

interface CellProps {
  items: BackpackItem[];
  columnCount: number;
  selected: Set<string>;
  onSelect: (assetId: string, additive: boolean) => void;
  onContextMenu: (assetId: string, x: number, y: number) => void;
}

function Cell({
  columnIndex,
  rowIndex,
  style,
  items,
  columnCount,
  selected,
  onSelect,
  onContextMenu,
}: CellComponentProps<CellProps>) {
  const index = rowIndex * columnCount + columnIndex;
  const item = items[index];
  if (!item) {
    return <div style={style} />;
  }
  return (
    <ItemTile
      item={item}
      isSelected={selected.has(item.asset_id)}
      style={style}
      onSelect={onSelect}
      onContextMenu={onContextMenu}
    />
  );
}

interface BackpackGridProps {
  items: BackpackItem[];
}

export function BackpackGrid({ items }: BackpackGridProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) {
        setSize({ width: entry.contentRect.width, height: entry.contentRect.height });
      }
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const selected = useBackpackStore((s) => s.selected);
  const selectItem = useBackpackStore((s) => s.selectItem);
  const openContextMenu = useBackpackStore((s) => s.openContextMenu);

  const columnCount = Math.max(1, Math.floor(size.width / TILE_SIZE));
  const rowCount = Math.max(1, Math.ceil(items.length / columnCount));

  const cellProps = useMemo<CellProps>(
    () => ({
      items,
      columnCount,
      selected,
      onSelect: selectItem,
      onContextMenu: (assetId: string, x: number, y: number) => openContextMenu({ assetId, x, y }),
    }),
    [items, columnCount, selected, selectItem, openContextMenu],
  );

  return (
    <div ref={containerRef} className="h-full w-full" data-testid="backpack-grid">
      {size.width > 0 && (
        <Grid
          cellComponent={Cell}
          cellProps={cellProps}
          columnCount={columnCount}
          columnWidth={TILE_SIZE}
          rowCount={rowCount}
          rowHeight={TILE_SIZE}
          defaultWidth={size.width}
          defaultHeight={size.height}
          style={{ width: size.width, height: size.height }}
          overscanCount={2}
        />
      )}
    </div>
  );
}
