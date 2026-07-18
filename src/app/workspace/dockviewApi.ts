import type { DockviewApi } from "dockview-react";

let dockviewApi: DockviewApi | null = null;

/** Set once from `WorkspaceShell`'s `onReady` — an imperative escape
 * hatch so components nested arbitrarily deep under it (e.g. a backpack
 * item tile) can open/focus a panel without prop-drilling the Dockview
 * API down through every intermediate layer. */
export function setDockviewApi(api: DockviewApi | null) {
  dockviewApi = api;
}

/** Opens `id` as a new panel titled `title` if it isn't already open, or
 * focuses it if it is. */
export function openOrFocusPanel(id: string, title: string) {
  if (!dockviewApi) return;
  const existing = dockviewApi.getPanel(id);
  if (existing) {
    existing.api.setActive();
  } else {
    dockviewApi.addPanel({ id, component: id, title });
  }
}
