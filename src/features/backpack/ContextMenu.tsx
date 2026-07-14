import { useEffect, useRef } from "react";
import { useBackpackStore } from "../../stores/backpackStore";
import {
  useAddItemTag,
  useCreateTag,
  useRemoveItemTag,
  useSetCustomLabel,
  useSetFavorite,
  useSetFolder,
  useSetNote,
  useSetPinned,
  useTags,
  type BackpackItem,
} from "./api";

interface ContextMenuProps {
  items: BackpackItem[];
}

export function ContextMenu({ items }: ContextMenuProps) {
  const contextMenu = useBackpackStore((s) => s.contextMenu);
  const closeContextMenu = useBackpackStore((s) => s.closeContextMenu);
  const selected = useBackpackStore((s) => s.selected);
  const menuRef = useRef<HTMLDivElement>(null);

  const setFavorite = useSetFavorite();
  const setPinned = useSetPinned();
  const setFolder = useSetFolder();
  const setNote = useSetNote();
  const setCustomLabel = useSetCustomLabel();
  const createTag = useCreateTag();
  const addItemTag = useAddItemTag();
  const removeItemTag = useRemoveItemTag();
  const { data: tags = [] } = useTags();

  useEffect(() => {
    if (!contextMenu) return;
    function onPointerDown(e: PointerEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        closeContextMenu();
      }
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") closeContextMenu();
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [contextMenu, closeContextMenu]);

  if (!contextMenu) return null;

  const targetIds =
    selected.has(contextMenu.assetId) && selected.size > 1
      ? Array.from(selected)
      : [contextMenu.assetId];
  const targetItems = items.filter((i) => targetIds.includes(i.asset_id));
  const primary = targetItems[0];
  if (!primary) return null;

  const allFavorite = targetItems.every((i) => i.meta.favorite);
  const allPinned = targetItems.every((i) => i.meta.pinned);

  function forEachTarget(fn: (assetId: string) => void) {
    targetIds.forEach(fn);
    closeContextMenu();
  }

  function handleSetFolder() {
    const value = window.prompt("Folder name (blank to clear):", primary.meta.folder ?? "");
    if (value === null) return;
    forEachTarget((assetId) => setFolder.mutate({ assetId, folder: value.trim() || null }));
  }

  function handleSetNote() {
    const value = window.prompt("Note (blank to clear):", primary.meta.note ?? "");
    if (value === null) return;
    forEachTarget((assetId) => setNote.mutate({ assetId, note: value.trim() || null }));
  }

  function handleSetCustomLabel() {
    const value = window.prompt("Custom label (blank to clear):", primary.meta.custom_label ?? "");
    if (value === null) return;
    forEachTarget((assetId) => setCustomLabel.mutate({ assetId, label: value.trim() || null }));
  }

  function handleNewTag() {
    const name = window.prompt("New tag name:");
    if (!name) return;
    const color = window.prompt("Tag color (hex):", "#8650AC") ?? "#8650AC";
    createTag.mutate(
      { name, color },
      {
        onSuccess: (tagId) => {
          forEachTarget((assetId) => addItemTag.mutate({ assetId, tagId }));
        },
      },
    );
  }

  return (
    <div
      ref={menuRef}
      className="fixed z-50 w-56 rounded-md border border-charcoal-border bg-charcoal py-1 text-sm text-zinc-200 shadow-xl"
      style={{ top: contextMenu.y, left: contextMenu.x }}
    >
      {targetIds.length > 1 && (
        <div className="border-b border-charcoal-border px-3 py-1 text-xs text-zinc-400">
          {targetIds.length} items selected
        </div>
      )}
      <MenuItem onClick={() => forEachTarget((assetId) => setFavorite.mutate({ assetId, favorite: !allFavorite }))}>
        {allFavorite ? "Unfavorite" : "Favorite"}
      </MenuItem>
      <MenuItem onClick={() => forEachTarget((assetId) => setPinned.mutate({ assetId, pinned: !allPinned }))}>
        {allPinned ? "Unpin" : "Pin"}
      </MenuItem>
      <MenuItem onClick={handleSetFolder}>Set folder…</MenuItem>
      <MenuItem onClick={handleSetCustomLabel}>Set custom label…</MenuItem>
      <MenuItem onClick={handleSetNote}>Set note…</MenuItem>
      <div className="border-t border-charcoal-border my-1" />
      <div className="px-3 py-1 text-xs text-zinc-400">Tags</div>
      {tags.map((tag) => {
        const hasTag = primary.tags.some((t) => t.id === tag.id);
        return (
          <MenuItem
            key={tag.id}
            onClick={() =>
              forEachTarget((assetId) =>
                hasTag
                  ? removeItemTag.mutate({ assetId, tagId: tag.id })
                  : addItemTag.mutate({ assetId, tagId: tag.id }),
              )
            }
          >
            <span className="mr-2 inline-block h-2.5 w-2.5 rounded-full align-middle" style={{ backgroundColor: tag.color }} />
            {tag.name} {hasTag ? "✓" : ""}
          </MenuItem>
        );
      })}
      <MenuItem onClick={handleNewTag}>New tag…</MenuItem>
    </div>
  );
}

function MenuItem({ children, onClick }: { children: React.ReactNode; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="block w-full px-3 py-1.5 text-left hover:bg-charcoal-raised"
    >
      {children}
    </button>
  );
}
