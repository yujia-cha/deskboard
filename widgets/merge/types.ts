/** 백엔드(`providers/merge`)가 주는 JSON 형태 — camelCase. */

export type Icon = { emoji: string } | { image: string } | { text: string };
export type ChainItem = { chain: string; level: number };
export type Item = { kind: "chain"; chain: string; level: number } | { kind: "gift"; items: ChainItem[] };
export type Cell =
  | { state: "empty" }
  | { state: "free"; item: Item }
  | { state: "web"; item: Item }
  | { state: "box"; item: Item };

export interface OrderView {
  resident: string;
  wants: { chain: string; level: number; have: boolean }[];
  ready: boolean;
}

export interface MergeView {
  /** 63칸, idx = y*7 + x */
  board: Cell[];
  inv: (Item | null)[];
  invSlots: number;
  invNextCost: number | null;
  pendingGifts: number;
  orders: OrderView[];
  stars: number;
  level: number;
  nextLevelAt: number | null;
  energy: number;
  /** unix ms */
  regenAnchor: number;
  clickerRem: number;
  keyClicker: boolean;
  keyNoticeSeen: boolean;
  tutorialStep: number;
  eco: {
    regenIntervalSec: number;
    regenCap: number;
    clicksPerEnergy: number;
    sellRefund: number;
    sellConfirmFromLevel: number;
    boxOpenCost: number;
    rerollCost: number;
    genCost: number;
  };
}

export interface MergeContent {
  chains: Record<string, {
    name: string;
    kind: "item" | "generator";
    maxLevel: number;
    unlockLevel: number;
    hidden: boolean;
    emits?: string[];
    levels: { name: string; icon: Icon }[];
  }>;
  residents: Record<string, { name: string; icon: Icon; likes: string[]; joinLevel: number; hidden: boolean }>;
  /** clicker, box, web, gift */
  skins: Record<string, Icon>;
}
