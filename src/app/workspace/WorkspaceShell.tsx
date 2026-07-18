import { useCallback, useEffect, useRef } from "react";
import { DockviewReact, type DockviewApi, type DockviewReadyEvent } from "dockview-react";
import "dockview/dist/styles/dockview.css";
import { useLogoutSteam } from "../../features/auth/api";
import { useSyncInventory } from "../../features/backpack/api";
import { useThemeStore, type ThemeName } from "../../stores/themeStore";
import {
  PANEL_TITLES,
  WORKSPACE_DEFAULTS,
  WORKSPACE_NAMES,
  useWorkspaceStore,
} from "../../stores/workspaceStore";
import { openOrFocusPanel, setDockviewApi } from "./dockviewApi";
import { panelComponents } from "./panels";

const LAYOUT_SAVE_DEBOUNCE_MS = 500;
const THEME_OPTIONS: ThemeName[] = ["dark", "light", "oled"];

function buildDefaultLayout(api: DockviewApi, name: string) {
  api.clear();
  const ids = WORKSPACE_DEFAULTS[name];
  ids.forEach((id, i) => {
    api.addPanel({
      id,
      component: id,
      title: PANEL_TITLES[id],
      ...(i === 0 ? {} : { position: { direction: "right" as const, referencePanel: ids[i - 1] } }),
    });
  });
}

function loadLayout(api: DockviewApi, name: string) {
  const saved = useWorkspaceStore.getState().layouts[name];
  if (saved) {
    api.fromJSON(saved);
  } else {
    buildDefaultLayout(api, name);
  }
}

export function WorkspaceShell({ steamId }: { steamId: string }) {
  const apiRef = useRef<DockviewApi | null>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const activeLayoutName = useWorkspaceStore((s) => s.activeLayoutName);
  const setActiveLayoutName = useWorkspaceStore((s) => s.setActiveLayoutName);
  const saveActiveLayout = useWorkspaceStore((s) => s.saveActiveLayout);
  const theme = useThemeStore((s) => s.theme);
  const setTheme = useThemeStore((s) => s.setTheme);
  const sync = useSyncInventory();
  const logout = useLogoutSteam();

  const openSettings = useCallback(() => {
    openOrFocusPanel("settings", PANEL_TITLES.settings);
  }, []);

  const switchWorkspace = useCallback(
    (name: string) => {
      setActiveLayoutName(name);
      if (apiRef.current) {
        loadLayout(apiRef.current, name);
      }
    },
    [setActiveLayoutName],
  );

  const onReady = useCallback(
    (event: DockviewReadyEvent) => {
      apiRef.current = event.api;
      setDockviewApi(event.api);
      loadLayout(event.api, activeLayoutName);
      event.api.onDidLayoutChange(() => {
        if (saveTimer.current) {
          clearTimeout(saveTimer.current);
        }
        saveTimer.current = setTimeout(() => {
          saveActiveLayout(event.api.toJSON());
        }, LAYOUT_SAVE_DEBOUNCE_MS);
      });
    },
    // Only re-run for a brand new Dockview instance — switching workspaces
    // afterwards goes through `switchWorkspace`, not a remount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  useEffect(() => () => setDockviewApi(null), []);

  // `1`-`9` workspace switch (docs/DESIGN.md §9), ignored while typing.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable)) {
        return;
      }
      const n = Number(e.key);
      if (n >= 1 && n <= 9 && WORKSPACE_NAMES[n - 1]) {
        switchWorkspace(WORKSPACE_NAMES[n - 1]);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [switchWorkspace]);

  return (
    <div className="flex h-screen w-screen flex-col bg-charcoal text-fg">
      <div className="flex items-center justify-between border-b border-charcoal-border px-4 py-2 text-sm">
        <div className="flex items-center gap-4">
          <span className="text-fg-muted">Steam: {steamId}</span>
          <nav className="flex gap-1" aria-label="Workspace">
            {WORKSPACE_NAMES.map((name) => (
              <button
                key={name}
                type="button"
                onClick={() => switchWorkspace(name)}
                className={`rounded px-3 py-1 ${
                  name === activeLayoutName ? "bg-charcoal-raised text-fg" : "text-fg-muted hover:text-fg"
                }`}
              >
                {name}
              </button>
            ))}
          </nav>
          <div className="flex gap-1 border-l border-charcoal-border pl-3" aria-label="Theme">
            {THEME_OPTIONS.map((t) => (
              <button
                key={t}
                type="button"
                onClick={() => setTheme(t)}
                title={`${t} theme`}
                className={`rounded px-2 py-1 capitalize ${
                  t === theme ? "bg-charcoal-raised text-fg" : "text-fg-muted hover:text-fg"
                }`}
              >
                {t}
              </button>
            ))}
          </div>
        </div>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={openSettings}
            className="rounded bg-charcoal-raised px-3 py-1 hover:bg-charcoal-border"
          >
            ⚙ Settings
          </button>
          <button
            type="button"
            onClick={() => sync.mutate()}
            disabled={sync.isPending}
            className="rounded bg-charcoal-raised px-3 py-1 hover:bg-charcoal-border disabled:opacity-50"
          >
            {sync.isPending ? "Syncing…" : "Sync Inventory"}
          </button>
          <button
            type="button"
            onClick={() => logout.mutate()}
            className="rounded bg-charcoal-raised px-3 py-1 hover:bg-charcoal-border"
          >
            Logout
          </button>
        </div>
      </div>

      {sync.isError && <p className="px-4 py-1 text-sm text-red-400">{sync.error.message}</p>}

      <div className="min-h-0 flex-1">
        <DockviewReact
          className={theme === "light" ? "dockview-theme-light" : "dockview-theme-dark"}
          components={panelComponents}
          onReady={onReady}
        />
      </div>
    </div>
  );
}
