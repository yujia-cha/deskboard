import type { Icon as IconData, Item, MergeContent } from "./types";

const UNKNOWN: IconData = { text: "?" };

/** `{emoji}` · `{text}` 는 글자로, `{image}` 는 <img> 로. */
export function Icon({ icon }: { icon: IconData | undefined }) {
  const i = icon ?? UNKNOWN;
  if ("image" in i) return <img className="merge-icon-img" src={i.image} alt="" draggable={false} />;
  return <span className="merge-icon-txt">{"emoji" in i ? i.emoji : i.text}</span>;
}

export function chainIcon(content: MergeContent | null, chain: string, level: number): IconData {
  return content?.chains[chain]?.levels[level - 1]?.icon ?? UNKNOWN;
}

export function isGenerator(content: MergeContent | null, item: Item | null): boolean {
  return !!item && item.kind === "chain" && content?.chains[item.chain]?.kind === "generator";
}

export function itemIcon(content: MergeContent | null, item: Item): IconData {
  return item.kind === "gift" ? (content?.skins.gift ?? { emoji: "🎁" }) : chainIcon(content, item.chain, item.level);
}

export function itemName(content: MergeContent | null, item: Item): string {
  if (item.kind === "gift") return "선물";
  return content?.chains[item.chain]?.levels[item.level - 1]?.name ?? "?";
}

/** 아이템 그림 + (생산기면) 레벨 배지 */
export function ItemView({ content, item }: { content: MergeContent | null; item: Item }) {
  return (
    <>
      <Icon icon={itemIcon(content, item)} />
      {item.kind === "chain" && isGenerator(content, item) && <span className="merge-badge">{item.level}</span>}
    </>
  );
}
