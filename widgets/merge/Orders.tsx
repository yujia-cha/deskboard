import type { MergeContent, MergeView } from "./types";
import { Icon, chainIcon } from "./Icon";

interface Props {
  view: MergeView;
  content: MergeContent | null;
  disabled: boolean;
  act: (cmd: string, args?: Record<string, unknown>) => Promise<boolean>;
}

export function Orders({ view, content, disabled, act }: Props) {
  return (
    <div className="merge-orders" style={disabled ? { pointerEvents: "none" } : undefined}>
      {view.orders.map((o, slot) => (
        <div key={slot} className={`merge-order ${o.ready ? "ready" : ""}`}>
          <div className="merge-order-wants">
            <span className="merge-order-who" title={content?.residents[o.resident]?.name ?? o.resident}>
              <Icon icon={content?.residents[o.resident]?.icon} />
            </span>
            {o.wants.map((w, i) => (
              <span key={i} className="merge-want" title={content?.chains[w.chain]?.levels[w.level - 1]?.name}>
                <Icon icon={chainIcon(content, w.chain, w.level)} />
                {w.have && <span className="merge-check">✔</span>}
              </span>
            ))}
          </div>
          <div className="merge-order-btns">
            <button className="merge-btn ok" disabled={!o.ready} onClick={() => act("merge_deliver", { slot })}>전달</button>
            <button className="merge-btn" title={`주문 바꾸기 −${view.eco.rerollCost}⚡`} onClick={() => act("merge_reroll", { slot })}>🔄</button>
          </div>
        </div>
      ))}
    </div>
  );
}
