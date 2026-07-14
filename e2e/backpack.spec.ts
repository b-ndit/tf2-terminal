import { expect, test } from "@playwright/test";
import { makeManyItems, mockTauri, mockTauriLoggedOut, SAMPLE_ITEMS } from "./fixtures";

test("shows the login screen when not logged in", async ({ page }) => {
  await mockTauriLoggedOut(page);
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Login with Steam" })).toBeVisible();
  await page.screenshot({ path: "e2e/screenshots/login.png" });
});

test("renders the backpack grid with items", async ({ page }) => {
  await mockTauri(page);
  await page.goto("/");
  await expect(page.getByTestId("backpack-grid")).toBeVisible();
  const tiles = page.getByTestId("item-tile");
  await expect(tiles).toHaveCount(SAMPLE_ITEMS.length);
  await expect(page.getByTestId("stats-total")).toHaveText(`Σ ${SAMPLE_ITEMS.length} items`);
  await page.screenshot({ path: "e2e/screenshots/backpack-grid.png" });
});

test("shows a tooltip with item details on hover", async ({ page }) => {
  await mockTauri(page);
  await page.goto("/");
  const strangeTile = page.getByTestId("item-tile").filter({ hasText: "Rocket Launcher" }).first();
  await strangeTile.hover();
  await expect(page.getByText("Strange count: 4210")).toBeVisible();
  await page.screenshot({ path: "e2e/screenshots/tooltip.png" });
});

test("opens a context menu on right-click with favorite/pin/tag actions", async ({ page }) => {
  await mockTauri(page);
  await page.goto("/");
  const tile = page.getByTestId("item-tile").first();
  await tile.click({ button: "right" });
  await expect(page.getByRole("button", { name: "Favorite" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Pin" })).toBeVisible();
  await expect(page.getByRole("button", { name: /trade-bait/ })).toBeVisible();
  await page.screenshot({ path: "e2e/screenshots/context-menu.png" });
});

test("ctrl-click selects multiple tiles for bulk context menu actions", async ({ page }) => {
  await mockTauri(page);
  await page.goto("/");
  const tiles = page.getByTestId("item-tile");
  await tiles.nth(0).click();
  await tiles.nth(1).click({ modifiers: ["Control"] });
  await tiles.nth(0).click({ button: "right" });
  await expect(page.getByText(/items selected/)).toBeVisible();
  await page.screenshot({ path: "e2e/screenshots/multi-select-context-menu.png" });
});

test("virtualizes a 3000-item backpack instead of rendering every tile", async ({ page }) => {
  const items = makeManyItems(3000);
  await mockTauri(page, items);
  await page.goto("/");
  await expect(page.getByTestId("stats-total")).toHaveText("Σ 3000 items");

  const renderedTiles = await page.getByTestId("item-tile").count();
  expect(renderedTiles).toBeGreaterThan(0);
  // Only the visible viewport (plus overscan) should be in the DOM, not all
  // 3000 — this is the whole point of virtualization.
  expect(renderedTiles).toBeLessThan(200);

  await page.screenshot({ path: "e2e/screenshots/virtualized-3000-items.png" });
});
