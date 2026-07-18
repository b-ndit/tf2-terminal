import { expect, test } from "@playwright/test";
import { mockTauri } from "./fixtures";

test("analyzes a classifieds URL and shows spread/liquidity/demand and buyer/seller tables", async ({ page }) => {
  await mockTauri(page);
  await page.goto("/");

  // Market Analyzer is one of the Dockview panels visible by default in
  // the "Trading" workspace (docs/DESIGN.md §9) — no tab click needed.
  await page.getByPlaceholder("Paste a backpack.tf classifieds URL…").fill(
    "https://backpack.tf/classifieds?item=Team+Captain&quality=5&particle=701",
  );
  await page.getByRole("button", { name: "Analyze", exact: true }).click();

  const result = page.getByTestId("analytics-result");
  await expect(result).toBeVisible();
  await expect(result).toContainText("Team Captain");
  await expect(result).toContainText("71"); // liquidity score
  await expect(result).toContainText("84"); // demand score
  await expect(page.getByText("Buyers (2)")).toBeVisible();
  await expect(page.getByText("Sellers (1)")).toBeVisible();
  await expect(page.getByText("Buyer One")).toBeVisible();
  await expect(page.getByText("Seller One")).toBeVisible();

  await expect(page.getByText("+1.1%")).toBeVisible(); // 1D trend
  await expect(page.getByTestId("price-chart")).toBeVisible();

  await page.screenshot({ path: "e2e/screenshots/market-analyzer.png" });
});

test("shows an error message for an unknown item", async ({ page }) => {
  await mockTauri(page);
  await page.goto("/");

  await page.getByPlaceholder("Paste a backpack.tf classifieds URL…").fill(
    "https://backpack.tf/classifieds?item=Nonexistent&quality=6",
  );
  await page.getByRole("button", { name: "Analyze", exact: true }).click();

  await expect(page.getByText(/unknown item/i)).toBeVisible();
});
