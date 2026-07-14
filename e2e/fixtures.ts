import type { Page } from "@playwright/test";

export const STEAM_ID = "76561198000000000";

export const SAMPLE_TAGS = [
  { id: 1, name: "trade-bait", color: "#cf6a32" },
  { id: 2, name: "keep", color: "#4d7455" },
];

function baseItem(overrides: Record<string, unknown>) {
  return {
    asset_id: "1",
    item_id: 1,
    name: "Team Captain",
    quality: 6,
    effect_id: null,
    killstreak_tier: 0,
    australium: false,
    festivized: false,
    craftable: true,
    craft_number: null,
    paint_id: null,
    strange_count: null,
    tradable: true,
    marketable: true,
    acquired_ts: 1700000000,
    last_seen_ts: 1700000000,
    meta: { folder: null, pinned: false, favorite: false, note: null, custom_label: null },
    tags: [],
    ...overrides,
  };
}

export const SAMPLE_ITEMS = [
  baseItem({ asset_id: "1", name: "Mann Co. Supply Crate Key", quality: 6 }),
  baseItem({
    asset_id: "2",
    name: "Team Captain",
    quality: 5,
    effect_id: 701,
    meta: { folder: null, pinned: false, favorite: true, note: null, custom_label: null },
  }),
  baseItem({
    asset_id: "3",
    name: "Rocket Launcher",
    quality: 11,
    strange_count: 4210,
  }),
  baseItem({
    asset_id: "4",
    name: "Scattergun",
    quality: 6,
    killstreak_tier: 3,
    tags: [SAMPLE_TAGS[0]],
  }),
  baseItem({
    asset_id: "5",
    name: "Rocket Launcher",
    quality: 6,
    australium: true,
    paint_id: 0x7e7e7e,
    meta: { folder: "Trade-up", pinned: true, favorite: false, note: "for trade", custom_label: null },
  }),
];

export function makeManyItems(count: number) {
  return Array.from({ length: count }, (_, i) =>
    baseItem({ asset_id: String(i + 1), name: `Item ${i + 1}`, quality: i % 16 }),
  );
}

export const SAMPLE_ANALYTICS = {
  item_name: "Team Captain",
  quality: 5,
  effect_id: 701,
  spread_abs_ref: 12.5,
  spread_pct: 4.2,
  liquidity_score: 71,
  demand_score: 84,
  estimated_sale_price_ref: 398.2,
  estimated_quicksell_ref: 380.0,
  buy_listings: [
    { listing_id: "b1", defindex: 378, steam_id: "s1", steam_name: "Buyer One", price_ref: 380.0, age_hours: 2.5 },
    { listing_id: "b2", defindex: 378, steam_id: "s2", steam_name: "Buyer Two", price_ref: 375.0, age_hours: 10.0 },
  ],
  sell_listings: [
    { listing_id: "s1", defindex: 378, steam_id: "s3", steam_name: "Seller One", price_ref: 405.0, age_hours: 1.0 },
  ],
  trend_ma7_ref: 392.1,
  trend_ma30_ref: 385.4,
  trend_volatility_pct: 3.2,
  trend_d1_pct: 1.1,
  trend_d7_pct: -2.4,
  trend_d30_pct: 5.6,
  trend_d365_pct: 18.9,
};

export const SAMPLE_PRICE_HISTORY = Array.from({ length: 14 }, (_, i) => {
  const day = 19_500 + i;
  const close = 380 + i * 1.5;
  return {
    ts: day * 86_400,
    open_ref: close - 1,
    high_ref: close + 3,
    low_ref: close - 3,
    close_ref: close,
    samples: 4,
  };
});

export async function mockTauri(
  page: Page,
  items: ReturnType<typeof baseItem>[] = SAMPLE_ITEMS,
  analytics: unknown = SAMPLE_ANALYTICS,
  priceHistory: unknown = SAMPLE_PRICE_HISTORY,
) {
  await page.addInitScript(
    ({ steamId, items, tags, analytics, priceHistory }) => {
      const responses: Record<string, (args: unknown) => unknown> = {
        get_steam_id: () => steamId,
        get_inventory: () => items,
        list_tags: () => tags,
        sync_inventory: () => ({
          added: 0,
          updated: 0,
          unchanged: items.length,
          removed: 0,
          total: items.length,
        }),
        login_with_steam: () => steamId,
        logout_steam: () => null,
        set_favorite: () => null,
        set_pinned: () => null,
        set_folder: () => null,
        set_note: () => null,
        set_custom_label: () => null,
        create_tag: () => 99,
        add_item_tag: () => null,
        remove_item_tag: () => null,
        analyze_classified_url: (args: unknown) => {
          const url = (args as { url?: string } | undefined)?.url ?? "";
          if (url.includes("Nonexistent")) {
            throw new Error("unknown item 'Nonexistent'");
          }
          return analytics;
        },
        get_price_history: (args: unknown) => {
          const url = (args as { url?: string } | undefined)?.url ?? "";
          if (url.includes("Nonexistent")) {
            throw new Error("unknown item 'Nonexistent'");
          }
          return priceHistory;
        },
      };
      // @ts-expect-error injected mock, not the real Tauri runtime
      window.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args: unknown) => {
          const handler = responses[cmd];
          if (!handler) throw new Error(`unmocked command: ${cmd}`);
          return handler(args);
        },
      };
    },
    { steamId: STEAM_ID, items, tags: SAMPLE_TAGS, analytics, priceHistory },
  );
}

export async function mockTauriLoggedOut(page: Page) {
  await page.addInitScript(() => {
    // @ts-expect-error injected mock, not the real Tauri runtime
    window.__TAURI_INTERNALS__ = {
      invoke: async (cmd: string) => {
        if (cmd === "get_steam_id") return null;
        throw new Error(`unmocked command: ${cmd}`);
      },
    };
  });
}
