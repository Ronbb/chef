import type { ReactNode } from "react";
import { Icon } from "./icon";

const labels = {
  explanation: "表达笔记",
  culture: "生活观察",
  vocabulary: "词汇收藏",
  grammar: "语法笔记",
  habit: "日常实践",
};

export function LessonNote({
  kind,
  title,
  children,
}: {
  kind: keyof typeof labels;
  title: string;
  children: ReactNode;
}) {
  return (
    <details className="lesson-note note-card" data-kind={kind}>
      <summary>
        <span className="note-card-heading">
          <span className="note-card-label">{labels[kind]}</span>
          <span className="note-card-title">{title}</span>
        </span>
        <span className="note-card-toggle" aria-hidden="true">
          <Icon name="plus" />
        </span>
      </summary>
      <div className="lesson-note-body">{children}</div>
    </details>
  );
}
