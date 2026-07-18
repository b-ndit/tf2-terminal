# Release / Packaging

Module 15 scope: a working local Linux build plus this checklist — not a
CI release pipeline. `.github/workflows/ci.yml` still only runs
lint/test/build on every push/PR; it does not build or publish installers.

## Prerequisites

Same system packages CI's backend job installs
(`.github/workflows/ci.yml`), plus `xdg-utils` for AppImage bundling
specifically (not needed for `.deb`):

```
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev \
  libayatana-appindicator3-dev librsvg2-dev xdg-utils
```

The Rust/Node toolchains themselves are whatever `rustup`/`node` already
have set up for local development — no extra packaging-specific toolchain.

## Version bump checklist

Three files carry the version number and must be kept in sync manually —
there's no single source of truth wired up yet:

- `package.json` → `"version"`
- `src-tauri/Cargo.toml` → `[package] version`
- `src-tauri/tauri.conf.json` → `"version"`

All three are currently `0.1.0`.

## Build

```
npm run tauri build
```

This runs `npm run build` (frontend) then a release-profile `cargo build`
and bundles the result. To build only specific targets (faster iteration,
skips whichever bundler you don't need locally):

```
npm run tauri build -- --bundles deb
npm run tauri build -- --bundles appimage
```

Artifacts land under `src-tauri/target/release/bundle/<format>/`:

- `deb/tf2-terminal_<version>_amd64.deb`
- `appimage/tf2-terminal_<version>_amd64.AppImage`

The `.deb` depends on `libwebkit2gtk-4.1-0` and `libgtk-3-0` at install
time (declared in the bundle's control file) — it does not vendor them.
The AppImage bundles its own runtime and has no such host dependency.

## Manual smoke test before tagging a release

1. Install/run the built artifact (`sudo dpkg -i <deb>` or
   `chmod +x <AppImage> && ./<AppImage>`) on a machine that isn't your dev
   box, if you have one — catches missing-dependency issues the dev
   environment already had installed.
2. Launch, log in with Steam, sync inventory.
3. Confirm the three seeded workspaces (Trading/Portfolio/Sniping) render
   and switch correctly, and that a rearranged layout persists across a
   restart.
4. Switch all three themes (dark/light/oled).
5. Export each dataset (Backpack, Trade History, Portfolio) in all four
   formats (CSV/XLSX/JSON/PDF) and open the resulting files.
6. Check `~/.local/share/tf2-terminal` (or platform equivalent —
   `infra::config::AppPaths`) for the log file and confirm no panics were
   logged during the above.
