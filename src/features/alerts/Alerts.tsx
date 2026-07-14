import { useState } from "react";
import { useAlertEvents, useAlertRules, useCreateAlertRule, useDeleteAlertRule, useSetAlertRuleEnabled } from "./api";

const KIND_OPTIONS = [
  { value: "price_drop", label: "Price drop below" },
  { value: "spread_widen", label: "Spread widens above" },
  { value: "new_buyer", label: "New buyer appears" },
  { value: "new_seller", label: "New seller appears" },
  { value: "hist_low", label: "New historical low" },
  { value: "hist_high", label: "New historical high" },
];
const THRESHOLD_KINDS = new Set(["price_drop", "spread_widen"]);
const CHANNEL_OPTIONS = ["desktop", "discord", "sound"] as const;

function kindLabel(kind: string): string {
  return KIND_OPTIONS.find((option) => option.value === kind)?.label ?? kind;
}

export function Alerts() {
  const [url, setUrl] = useState("");
  const [kind, setKind] = useState("price_drop");
  const [threshold, setThreshold] = useState("");
  const [channels, setChannels] = useState<string[]>(["desktop"]);

  const rules = useAlertRules();
  const create = useCreateAlertRule();
  const setEnabled = useSetAlertRuleEnabled();
  const del = useDeleteAlertRule();
  const { recent } = useAlertEvents();

  const needsThreshold = THRESHOLD_KINDS.has(kind);

  function toggleChannel(channel: string) {
    setChannels((prev) => (prev.includes(channel) ? prev.filter((c) => c !== channel) : [...prev, channel]));
  }

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = url.trim();
    if (!trimmed || (needsThreshold && threshold.trim() === "")) return;
    create.mutate(
      { url: trimmed, kind, threshold: needsThreshold ? Number(threshold) : null, channels },
      { onSuccess: () => setUrl("") },
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto bg-charcoal p-4 text-zinc-200">
      <h2 className="mb-4 text-lg font-semibold">Alerts</h2>

      <form
        onSubmit={handleSubmit}
        className="mb-6 flex flex-wrap items-end gap-3 rounded border border-charcoal-border bg-charcoal-raised p-3"
      >
        <div className="flex min-w-[240px] flex-1 flex-col gap-1">
          <label className="text-xs text-zinc-400">Item (classifieds URL)</label>
          <input
            type="text"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="Paste a backpack.tf classifieds URL…"
            className="rounded border border-charcoal-border bg-charcoal px-3 py-2 text-sm placeholder:text-zinc-500 focus:outline-none"
          />
        </div>

        <div className="flex flex-col gap-1">
          <label className="text-xs text-zinc-400">Kind</label>
          <select
            value={kind}
            onChange={(e) => setKind(e.target.value)}
            className="rounded border border-charcoal-border bg-charcoal px-3 py-2 text-sm"
          >
            {KIND_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </div>

        {needsThreshold && (
          <div className="flex flex-col gap-1">
            <label className="text-xs text-zinc-400">Threshold ({kind === "spread_widen" ? "%" : "ref"})</label>
            <input
              type="number"
              step="0.01"
              value={threshold}
              onChange={(e) => setThreshold(e.target.value)}
              className="w-28 rounded border border-charcoal-border bg-charcoal px-3 py-2 text-sm"
            />
          </div>
        )}

        <div className="flex flex-col gap-1">
          <label className="text-xs text-zinc-400">Channels</label>
          <div className="flex gap-3 py-2">
            {CHANNEL_OPTIONS.map((channel) => (
              <label key={channel} className="flex items-center gap-1 text-sm capitalize">
                <input type="checkbox" checked={channels.includes(channel)} onChange={() => toggleChannel(channel)} />
                {channel}
              </label>
            ))}
          </div>
        </div>

        <button
          type="submit"
          disabled={create.isPending || !url.trim() || (needsThreshold && threshold.trim() === "")}
          className="rounded bg-quality-unique px-4 py-2 text-sm font-medium text-black hover:opacity-90 disabled:opacity-50"
        >
          {create.isPending ? "Adding…" : "Add Alert"}
        </button>
      </form>

      {create.isError && (
        <p className="mb-4 rounded border border-red-900 bg-red-950/40 px-3 py-2 text-sm text-red-400">{create.error.message}</p>
      )}

      <div className="mb-6">
        <h3 className="mb-2 text-sm font-medium text-zinc-400">Rules ({rules.data?.length ?? 0})</h3>
        <div className="rounded border border-charcoal-border">
          {(rules.data ?? []).length === 0 ? (
            <p className="px-3 py-3 text-sm text-zinc-500">No alert rules yet.</p>
          ) : (
            rules.data!.map((rule) => (
              <div
                key={rule.id}
                className="flex flex-wrap items-center justify-between gap-2 border-b border-charcoal-border px-3 py-2 text-sm last:border-0"
              >
                <div className="min-w-0">
                  <span className="font-medium">{rule.item_name}</span>
                  <span className="ml-2 text-xs text-zinc-400">
                    {kindLabel(rule.kind)}
                    {rule.threshold !== null && ` ${rule.threshold}`}
                  </span>
                  <span className="ml-2 text-xs text-zinc-500">[{rule.channels.join(", ")}]</span>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  <button
                    type="button"
                    onClick={() => setEnabled.mutate({ ruleId: rule.id, enabled: !rule.enabled })}
                    className={`rounded px-2 py-1 text-xs ${
                      rule.enabled ? "bg-charcoal-raised hover:bg-charcoal-border" : "bg-charcoal-raised text-zinc-500 hover:bg-charcoal-border"
                    }`}
                  >
                    {rule.enabled ? "Enabled" : "Disabled"}
                  </button>
                  <button
                    type="button"
                    onClick={() => del.mutate(rule.id)}
                    className="rounded bg-charcoal-raised px-2 py-1 text-xs text-red-400 hover:bg-charcoal-border"
                  >
                    Delete
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>

      <div>
        <h3 className="mb-2 text-sm font-medium text-zinc-400">Recent Alerts</h3>
        <div className="rounded border border-charcoal-border">
          {recent.length === 0 ? (
            <p className="px-3 py-3 text-sm text-zinc-500">No alerts fired yet.</p>
          ) : (
            recent.map((event, index) => (
              <div key={index} className="border-b border-charcoal-border px-3 py-2 text-sm last:border-0">
                {event.message}
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
