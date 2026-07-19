import { useEffect, useState } from "react";
import { useSendOfferStore } from "../../stores/sendOfferStore";
import { useHasSteamSession } from "../settings/api";
import { ItemIcon } from "../backpack/ItemIcon";
import { qualityColor } from "../backpack/quality";
import { usePublicInventory, useSendTradeOffer } from "./api";
import type { PartnerItemView } from "./api";

/**
 * Deliberately minimal "propose a trade" flow — not the fuller drag-drop
 * builder `docs/DESIGN.md` already scoped out of Module 13. "You give" is
 * fixed to whatever was multi-selected in the Backpack before opening this
 * (`ContextMenu`'s "Propose Trade…"); "you want" is picked from a live,
 * read-only fetch of the partner's public inventory once a SteamID64 is
 * entered.
 */
export function SendOfferForm() {
  const giveItems = useSendOfferStore((s) => s.giveItems);
  const close = useSendOfferStore((s) => s.close);
  const { data: sessionConnected } = useHasSteamSession();
  const [partnerSteamId, setPartnerSteamId] = useState("");
  const [wantAssetIds, setWantAssetIds] = useState<Set<string>>(new Set());
  const [message, setMessage] = useState("");
  const [confirming, setConfirming] = useState(false);
  const wantInventory = usePublicInventory(partnerSteamId);
  const send = useSendTradeOffer();

  useEffect(() => {
    if (!giveItems) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") handleClose();
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [giveItems]);

  if (!giveItems) return null;

  function toggleWant(assetId: string) {
    setWantAssetIds((prev) => {
      const next = new Set(prev);
      if (next.has(assetId)) next.delete(assetId);
      else next.add(assetId);
      return next;
    });
  }

  function handleClose() {
    setPartnerSteamId("");
    setWantAssetIds(new Set());
    setMessage("");
    setConfirming(false);
    send.reset();
    close();
  }

  function handleSend() {
    send.mutate(
      {
        partnerSteamId,
        myAssetIds: giveItems!.map((i) => i.asset_id),
        theirAssetIds: Array.from(wantAssetIds),
        message,
      },
      { onSuccess: handleClose },
    );
  }

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 p-4" onClick={handleClose}>
      <div
        className="max-h-[85vh] w-full max-w-lg overflow-y-auto rounded-lg border border-charcoal-border bg-charcoal-raised p-4 text-fg shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-3 flex items-center justify-between">
          <h3 className="font-semibold">Propose Trade</h3>
          <button type="button" onClick={handleClose} aria-label="Close" className="rounded px-2 py-1 text-fg-muted hover:bg-charcoal-border">
            ✕
          </button>
        </div>

        {sessionConnected === false && (
          <p className="mb-3 rounded border border-amber-900 bg-amber-950/30 px-3 py-2 text-xs text-amber-300">
            Connect a Steam session in Settings before sending a real trade offer.
          </p>
        )}

        <label className="mb-1 block text-xs text-fg-muted">Partner SteamID64</label>
        <input
          type="text"
          value={partnerSteamId}
          onChange={(e) => setPartnerSteamId(e.target.value.trim())}
          placeholder="76561198000000000"
          className="mb-3 w-full rounded border border-charcoal-border bg-charcoal px-2 py-1 text-sm placeholder:text-fg-subtle"
        />

        <div className="mb-3">
          <div className="mb-1 text-xs font-medium text-fg-muted">You Give ({giveItems.length})</div>
          <ul className="max-h-32 divide-y divide-charcoal-border overflow-y-auto rounded border border-charcoal-border">
            {giveItems.map((item) => (
              <li key={item.asset_id} className="flex items-center gap-2 px-2 py-1 text-sm">
                <ItemIcon imageUrl={item.image_url} alt={item.name} size="h-6 w-6" />
                <span style={{ color: qualityColor(item.quality) }}>{item.meta.custom_label ?? item.name}</span>
              </li>
            ))}
          </ul>
        </div>

        <div className="mb-3">
          <div className="mb-1 text-xs font-medium text-fg-muted">
            You Want ({wantAssetIds.size} selected)
          </div>
          <div className="max-h-40 overflow-y-auto rounded border border-charcoal-border">
            {partnerSteamId.length === 0 ? (
              <p className="px-2 py-2 text-xs text-fg-subtle">Enter a partner SteamID64 to browse their inventory.</p>
            ) : wantInventory.isLoading ? (
              <p className="px-2 py-2 text-xs text-fg-subtle">Loading their inventory…</p>
            ) : wantInventory.isError ? (
              <p className="px-2 py-2 text-xs text-red-400">{wantInventory.error.message}</p>
            ) : (wantInventory.data ?? []).length === 0 ? (
              <p className="px-2 py-2 text-xs text-fg-subtle">No items found (inventory may be private).</p>
            ) : (
              <ul className="divide-y divide-charcoal-border">
                {wantInventory.data!.map((item: PartnerItemView) => (
                  <li key={item.asset_id}>
                    <button
                      type="button"
                      onClick={() => toggleWant(item.asset_id)}
                      className={`flex w-full items-center gap-2 px-2 py-1 text-left text-sm hover:bg-charcoal-border ${
                        wantAssetIds.has(item.asset_id) ? "bg-charcoal-border" : ""
                      }`}
                    >
                      <ItemIcon imageUrl={item.image_url} alt={item.name} size="h-6 w-6" />
                      <span style={{ color: qualityColor(item.quality) }}>{item.name}</span>
                      {wantAssetIds.has(item.asset_id) && <span className="ml-auto text-xs text-quality-genuine">✓</span>}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>

        <label className="mb-1 block text-xs text-fg-muted">Message (optional)</label>
        <textarea
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          rows={2}
          className="mb-3 w-full rounded border border-charcoal-border bg-charcoal px-2 py-1 text-sm placeholder:text-fg-subtle"
        />

        {send.isError && <p className="mb-2 text-xs text-red-400">{send.error.message}</p>}

        {confirming ? (
          <div className="flex items-center gap-2">
            <span className="text-xs text-amber-300">Send this real trade offer?</span>
            <button
              type="button"
              disabled={send.isPending}
              onClick={handleSend}
              className="rounded bg-quality-genuine px-3 py-1 text-sm font-medium text-black hover:opacity-90 disabled:opacity-50"
            >
              {send.isPending ? "Sending…" : "Confirm Send"}
            </button>
            <button type="button" onClick={() => setConfirming(false)} className="rounded bg-charcoal-border px-3 py-1 text-sm hover:opacity-90">
              Cancel
            </button>
          </div>
        ) : (
          <button
            type="button"
            disabled={!partnerSteamId || sessionConnected === false}
            onClick={() => setConfirming(true)}
            className="w-full rounded bg-quality-unique px-3 py-2 text-sm font-medium text-black hover:opacity-90 disabled:opacity-50"
          >
            Send Offer
          </button>
        )}
      </div>
    </div>
  );
}
