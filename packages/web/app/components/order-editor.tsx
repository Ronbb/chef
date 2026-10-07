import { useLayoutEffect, useRef } from "react";

export function OrderEditor({
  tokens,
  order,
  onChange,
}: {
  tokens: { id: string; text: string }[];
  order: string[];
  onChange: (order: string[]) => void;
}) {
  const bank = useRef(new Map<string, HTMLButtonElement>()),
    sentence = useRef(new Map<string, HTMLButtonElement>()),
    focus = useRef<{ area: "bank" | "sentence"; id: string } | null>(null);
  useLayoutEffect(() => {
    const target = focus.current;
    focus.current = null;
    if (!target) return;
    const button = (target.area === "bank" ? bank : sentence).current.get(
      target.id,
    );
    if (button && !button.matches(":disabled"))
      button.focus({ preventScroll: true });
  }, [order]);
  return (
    <>
      <div className="order-answer" role="group" aria-label="当前句子">
        {order.length ? (
          order.map((id) => (
            <button
              key={id}
              type="button"
              ref={(element) => {
                if (element) sentence.current.set(id, element);
                else sentence.current.delete(id);
              }}
              aria-label={
                "移回词库：" + tokens.find((token) => token.id === id)?.text
              }
              onClick={() => {
                focus.current = { area: "bank", id };
                onChange(order.filter((value) => value !== id));
              }}
            >
              <span lang="fr">
                {tokens.find((token) => token.id === id)?.text}
              </span>
            </button>
          ))
        ) : (
          <span className="order-empty">组成一句话</span>
        )}
      </div>
      <div className="order-bank" role="group" aria-label="词库">
        {tokens.map((token) => (
          <button
            key={token.id}
            type="button"
            ref={(element) => {
              if (element) bank.current.set(token.id, element);
              else bank.current.delete(token.id);
            }}
            disabled={order.includes(token.id)}
            onClick={() => {
              const next = [...order, token.id];
              const available = tokens.filter(
                (item) => !next.includes(item.id),
              );
              const following =
                available.find(
                  (item) => tokens.indexOf(item) > tokens.indexOf(token),
                ) ?? available[0];
              focus.current = following
                ? { area: "bank", id: following.id }
                : { area: "sentence", id: token.id };
              onChange(next);
            }}
          >
            <span lang="fr">{token.text}</span>
          </button>
        ))}
      </div>
      <span className="sr-only" role="status" lang="fr">
        {order
          .map((id) => tokens.find((token) => token.id === id)?.text)
          .join(" ")}
      </span>
    </>
  );
}
