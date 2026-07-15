import { convertFileSrc } from "@tauri-apps/api/core";
import { useState } from "react";
import {
  useInstallPlugin,
  useListPlugins,
  usePluginPanelPath,
  useSetPluginEnabled,
  useUninstallPlugin,
} from "./api";
import type { PluginSummary } from "./api";

export function Plugins() {
  const { data: plugins = [], isLoading, error } = useListPlugins();
  const install = useInstallPlugin();
  const [sourceDir, setSourceDir] = useState("");

  function handleInstall() {
    if (sourceDir.trim() === "") return;
    install.mutate(sourceDir.trim(), {
      onSuccess: () => setSourceDir(""),
    });
  }

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto bg-charcoal p-4 text-zinc-200">
      <h2 className="mb-4 text-lg font-semibold">Plugins</h2>

      <div className="mb-4 rounded border border-charcoal-border bg-charcoal-raised p-3">
        <div className="mb-2 text-sm font-medium">Install a plugin</div>
        <p className="mb-2 text-xs text-zinc-500">
          Enter the path to a local folder containing <code>plugin.toml</code> and its wasm entry file. Installing runs
          the plugin with exactly the capabilities its manifest requests — check{" "}
          <code>plugin.toml</code> before installing anything you don't trust.
        </p>
        <div className="flex gap-2">
          <input
            type="text"
            value={sourceDir}
            onChange={(e) => setSourceDir(e.target.value)}
            placeholder="/path/to/plugin-folder"
            className="flex-1 rounded border border-charcoal-border bg-charcoal px-2 py-1.5 text-sm placeholder:text-zinc-500 focus:outline-none"
          />
          <button
            type="button"
            onClick={handleInstall}
            disabled={install.isPending || sourceDir.trim() === ""}
            className="rounded bg-quality-unique px-3 py-1.5 text-sm font-medium text-black hover:opacity-90 disabled:opacity-50"
          >
            {install.isPending ? "Installing…" : "Install"}
          </button>
        </div>
        {install.isError && <p className="mt-2 text-sm text-red-400">{install.error.message}</p>}
      </div>

      {error && <p className="text-sm text-red-400">{error.message}</p>}
      {isLoading ? (
        <p className="text-sm text-zinc-500">Loading…</p>
      ) : plugins.length === 0 ? (
        <p className="text-sm text-zinc-500">No plugins installed yet.</p>
      ) : (
        <div className="flex flex-col gap-3">
          {plugins.map((plugin) => (
            <PluginCard key={plugin.name} plugin={plugin} />
          ))}
        </div>
      )}
    </div>
  );
}

function PluginCard({ plugin }: { plugin: PluginSummary }) {
  const setEnabled = useSetPluginEnabled();
  const uninstall = useUninstallPlugin();
  const [panelOpen, setPanelOpen] = useState(false);
  const panel = usePluginPanelPath(plugin.name, panelOpen && plugin.has_panel);

  return (
    <div className="rounded border border-charcoal-border bg-charcoal-raised p-3">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-sm font-medium">
            {plugin.name} <span className="text-xs text-zinc-500">v{plugin.version}</span>
          </div>
          <div className="mt-1 flex flex-wrap gap-1">
            {plugin.capabilities.map((cap) => (
              <span key={cap} className="rounded bg-charcoal px-1.5 py-0.5 text-[10px] text-zinc-400">
                {cap}
              </span>
            ))}
            {plugin.events.map((event) => (
              <span key={event} className="rounded bg-charcoal px-1.5 py-0.5 text-[10px] text-quality-unique">
                {event}
              </span>
            ))}
          </div>
        </div>
        <div className="flex items-center gap-2">
          <label className="flex items-center gap-1 text-xs text-zinc-400">
            <input
              type="checkbox"
              checked={plugin.enabled}
              onChange={(e) => setEnabled.mutate({ name: plugin.name, enabled: e.target.checked })}
            />
            Enabled
          </label>
          {plugin.has_panel && (
            <button
              type="button"
              onClick={() => setPanelOpen((v) => !v)}
              className="rounded bg-charcoal px-2 py-1 text-xs hover:bg-charcoal-border"
            >
              {panelOpen ? "Hide panel" : "Show panel"}
            </button>
          )}
          <button
            type="button"
            onClick={() => uninstall.mutate(plugin.name)}
            disabled={uninstall.isPending}
            className="rounded bg-charcoal px-2 py-1 text-xs text-red-400 hover:bg-charcoal-border disabled:opacity-50"
          >
            Uninstall
          </button>
        </div>
      </div>

      {setEnabled.isError && <p className="mt-2 text-xs text-red-400">{setEnabled.error.message}</p>}
      {uninstall.isError && <p className="mt-2 text-xs text-red-400">{uninstall.error.message}</p>}

      {panelOpen && plugin.has_panel && (
        <div className="mt-3 overflow-hidden rounded border border-charcoal-border">
          {panel.data ? (
            <iframe
              title={`${plugin.name} panel`}
              src={convertFileSrc(panel.data)}
              sandbox="allow-scripts"
              className="h-64 w-full bg-charcoal"
            />
          ) : (
            <p className="p-3 text-xs text-zinc-500">Loading panel…</p>
          )}
        </div>
      )}
    </div>
  );
}
