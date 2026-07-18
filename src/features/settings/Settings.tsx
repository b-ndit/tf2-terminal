import { useState } from "react";
import { useClearSecret, useHasSecret, useSetSecret, useSyncItemSchema, type SecretKind } from "./api";

interface SecretFieldConfig {
  kind: SecretKind;
  label: string;
  placeholder: string;
  helpText: string;
  helpUrl: string;
}

const FIELDS: SecretFieldConfig[] = [
  {
    kind: "steamApiKey",
    label: "Steam Web API Key",
    placeholder: "Paste your Steam Web API key",
    helpText: "Required to sync your inventory and poll trade offers. Get one at steamcommunity.com/dev/apikey (domain can be \"localhost\").",
    helpUrl: "https://steamcommunity.com/dev/apikey",
  },
  {
    kind: "backpackTfToken",
    label: "backpack.tf API Token",
    placeholder: "Paste your backpack.tf token",
    helpText: "Optional — unlocks account-scoped backpack.tf v2 endpoints. Get one from your backpack.tf settings page.",
    helpUrl: "https://backpack.tf/developer/apikey/view",
  },
  {
    kind: "discordWebhookUrl",
    label: "Discord Webhook URL",
    placeholder: "https://discord.com/api/webhooks/...",
    helpText: "Optional — lets Alerts post to a Discord channel. Create one in a channel's Integrations settings.",
    helpUrl: "https://support.discord.com/hc/en-us/articles/228383668",
  },
];

export function Settings() {
  return (
    <div className="flex h-full min-h-0 flex-col gap-4 overflow-y-auto bg-charcoal p-4 text-fg">
      <h2 className="text-lg font-semibold">Settings</h2>
      <p className="text-sm text-fg-muted">
        Secrets are stored in your OS keychain (or the Linux kernel keyring if no Secret Service is
        reachable) — never in the database, config file, or logs. Values are write-only here; once saved
        you can only replace or clear them, not view them again.
      </p>
      {FIELDS.map((field) => (
        <SecretField key={field.kind} {...field} />
      ))}
      <SchemaSyncField />
    </div>
  );
}

function SchemaSyncField() {
  const sync = useSyncItemSchema();

  return (
    <div className="rounded border border-charcoal-border bg-charcoal-raised p-3">
      <span className="font-medium">Item Schema</span>
      <p className="mb-2 text-xs text-fg-muted">
        Item names and icons come from Valve's schema, not your inventory sync — without this, items
        show as "Unknown Item" with no icon. Requires the Steam Web API key above. Refreshes weekly on
        its own; run manually to fix already-synced items right away.
      </p>
      <button
        type="button"
        disabled={sync.isPending}
        onClick={() => sync.mutate()}
        className="rounded bg-quality-unique px-3 py-1 text-sm font-medium text-black hover:opacity-90 disabled:opacity-50"
      >
        {sync.isPending ? "Syncing…" : "Sync Item Schema"}
      </button>
      {sync.isSuccess && (
        <p className="mt-2 text-xs text-quality-genuine">
          Synced {sync.data.items_synced} items ({sync.data.items_in_db} in database
          {sync.data.unknown_names_fixed > 0 && `, ${sync.data.unknown_names_fixed} previously-unknown fixed`}).
        </p>
      )}
      {sync.isError && <p className="mt-2 text-xs text-red-400">{sync.error.message}</p>}
    </div>
  );
}

function SecretField({ kind, label, placeholder, helpText, helpUrl }: SecretFieldConfig) {
  const { data: hasSecret, isLoading } = useHasSecret(kind);
  const setSecret = useSetSecret(kind);
  const clearSecret = useClearSecret(kind);
  const [value, setValue] = useState("");

  return (
    <div className="rounded border border-charcoal-border bg-charcoal-raised p-3">
      <div className="mb-1 flex items-center justify-between">
        <span className="font-medium">{label}</span>
        {!isLoading && (
          <span className={hasSecret ? "text-quality-genuine text-xs" : "text-fg-subtle text-xs"}>
            {hasSecret ? "Configured ✓" : "Not set"}
          </span>
        )}
      </div>
      <p className="mb-2 text-xs text-fg-muted">
        {helpText}{" "}
        <a href={helpUrl} target="_blank" rel="noreferrer" className="text-quality-unique underline">
          {helpUrl}
        </a>
      </p>
      <div className="flex gap-2">
        <input
          type="password"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder={placeholder}
          className="min-w-0 flex-1 rounded border border-charcoal-border bg-charcoal px-2 py-1 text-sm text-fg placeholder:text-fg-subtle"
        />
        <button
          type="button"
          disabled={!value || setSecret.isPending}
          onClick={() =>
            setSecret.mutate(value, {
              onSuccess: () => setValue(""),
            })
          }
          className="rounded bg-quality-unique px-3 py-1 text-sm font-medium text-black hover:opacity-90 disabled:opacity-50"
        >
          {setSecret.isPending ? "Saving…" : "Save"}
        </button>
        {hasSecret && (
          <button
            type="button"
            disabled={clearSecret.isPending}
            onClick={() => clearSecret.mutate()}
            className="rounded bg-charcoal-border px-3 py-1 text-sm hover:opacity-90 disabled:opacity-50"
          >
            Clear
          </button>
        )}
      </div>
      {setSecret.isError && <p className="mt-1 text-xs text-red-400">{setSecret.error.message}</p>}
      {clearSecret.isError && <p className="mt-1 text-xs text-red-400">{clearSecret.error.message}</p>}
    </div>
  );
}
