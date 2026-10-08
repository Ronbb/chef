import { Form, Link, useNavigation } from "react-router";
import { useEffect, useRef } from "react";
import { getReadingCatalog } from "../lib/api.server";
import type { Catalog } from "@brioche/contracts/Catalog";
import type { NeutralCatalog } from "@brioche/contracts/NeutralCatalog";
import { titleText } from "../lib/reading-model";
import product from "@chef/product";
import type { Route } from "./+types/courses";

export async function loader({ request }: Route.LoaderArgs) {
  const query = new URL(request.url).searchParams.get("q")?.trim() ?? "";
  if ([...query].length > 120)
    throw new Response("搜索内容请控制在 120 字以内。", { status: 400 });
  return { catalog: await getReadingCatalog(query), query };
}
export default function Courses({
  loaderData: { catalog, query },
}: Route.ComponentProps) {
  return <CoursesContent catalog={catalog} query={query} />;
}
export function CoursesContent({
  catalog,
  query,
}: {
  catalog: Catalog | NeutralCatalog;
  query: string;
}) {
  const navigation = useNavigation();
  const field = useRef<HTMLInputElement>(null);
  const submitted = useRef<string | null>(null);
  useEffect(() => {
    if (navigation.state !== "idle") return;
    if (
      field.current &&
      (submitted.current === null || field.current.value === submitted.current)
    )
      field.current.value = query;
    submitted.current = null;
  }, [query, navigation.state]);
  const count = catalog.levels.reduce(
    (total, level) =>
      total + level.units.reduce((n, unit) => n + unit.lessons.length, 0),
    0,
  );
  return (
    <section className="page-arrive courses-page">
      <div className="section-head">
        <h1>课程</h1>
        <Link className="text-button" to="/">
          回到首页
        </Link>
      </div>
      <Form
        method="get"
        action="/courses"
        className="course-search"
        role="search"
        onSubmit={(event) => {
          if (navigation.state !== "idle") event.preventDefault();
          else submitted.current = field.current?.value ?? "";
        }}
      >
        <label htmlFor="course-query">找一个场景或表达</label>
        <div className="course-search-field">
          <input
            ref={field}
            id="course-query"
            name="q"
            type="search"
            maxLength={120}
            defaultValue={query}
            placeholder={
              product.targetLanguage === "fr-FR"
                ? "早餐、面包店、bonjour…"
                : "飲茶、點心、nei5 hou2…"
            }
          />
          <button
            type="submit"
            className="text-button"
            aria-disabled={navigation.state !== "idle"}
            aria-busy={navigation.state !== "idle"}
          >
            搜索
          </button>
        </div>
      </Form>
      <p className="meta" role="status" aria-live="polite">
        {navigation.state !== "idle"
          ? "正在查找…"
          : query
            ? `找到 ${count} 堂课程`
            : `${count} 堂课程`}
      </p>
      {catalog.levels.map((level) => (
        <section key={level.id} className="course-level">
          <p className="eyebrow">{level.label}</p>
          {level.units.map((unit) => (
            <div key={unit.id} className="course-unit">
              <h2>{unit.titleZh}</h2>
              {unit.lessons.map((lesson, index) => (
                <Link
                  key={lesson.id}
                  className="lesson-row current"
                  to={"/lessons/" + lesson.id}
                >
                  <span className="lesson-number">
                    {String(index + 1).padStart(2, "0")}
                  </span>
                  <span className="lesson-label">
                    <b>{lesson.title.zh}</b>
                    <small
                      lang={
                        "targetLanguage" in lesson
                          ? lesson.targetLanguage
                          : "fr-FR"
                      }
                    >
                      {titleText(lesson.title)}
                    </small>
                    <small>{lesson.summaryZh}</small>
                  </span>
                </Link>
              ))}
            </div>
          ))}
        </section>
      ))}
      {!count && (
        <div className="empty-state">
          <h2>{query ? "还没有找到这个场景" : "课程正在准备中"}</h2>
          <p>
            {query
              ? "试试中文场景、课程表达或中文词义。"
              : "发布课程后，就可以开始学习。"}
          </p>
          {query && (
            <Link className="text-button" to="/courses">
              查看全部课程
            </Link>
          )}
        </div>
      )}
    </section>
  );
}
