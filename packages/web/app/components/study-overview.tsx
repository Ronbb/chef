import { Link } from "react-router";
import type { StudyDashboard } from "@brioche/contracts/StudyDashboard";
import type { NeutralStudyDashboard } from "@brioche/contracts/NeutralStudyDashboard";
import { Icon } from "./icon";
export function StudyOverview({
  dashboard,
}: {
  dashboard: StudyDashboard | NeutralStudyDashboard;
}) {
  const labels = ["一", "二", "三", "四", "五", "六", "日"];
  return (
    <div className="study-overview">
      <section className="study-week" aria-labelledby="study-week-title">
        <div className="study-week-heading">
          <h2 id="study-week-title">这一周</h2>
          <Link to="/profile" className="text-button">
            {dashboard.activeDays} / {dashboard.weeklyGoalDays} 天
          </Link>
        </div>
        <ol className="study-week-days">
          {dashboard.days.map((day, index) => (
            <li
              key={day.localDate}
              className={
                (day.active ? "is-active " : "") +
                (day.localDate === dashboard.localDate ? "is-today " : "") +
                (day.localDate > dashboard.localDate ? "is-future" : "")
              }
              aria-label={
                day.localDate +
                (day.active
                  ? `：确认 ${day.confirmedSteps} 个步骤，提交 ${day.exerciseAttempts} 次练习，复习 ${day.reviewAttempts} 个表达，首次完成 ${day.completedLessons} 课`
                  : day.localDate > dashboard.localDate
                    ? "：尚未到来"
                    : "：暂无学习记录")
              }
            >
              <span>周{labels[index]}</span>
              <span className="study-day-mark" aria-hidden="true">
                {day.active ? <Icon name="check" /> : day.localDate.slice(-2)}
              </span>
            </li>
          ))}
        </ol>
        <p className="study-week-note">
          每天 {dashboard.dailyGoalMinutes} 分钟目标 · 已完成{" "}
          {dashboard.completedLessons} 课
        </p>
      </section>
      <Link className="study-review-entry" to="/reviews">
        <span>
          <small>复习表达</small>
          <strong>
            {dashboard.dueReviews
              ? `${dashboard.dueReviews} 个表达已到期`
              : "暂时没有到期表达"}
          </strong>
          <span>
            {dashboard.dueReviews
              ? `本轮 ${Math.min(dashboard.dueReviews, 10)} 个表达`
              : dashboard.nextReviewAt
                ? "下次 " +
                  new Date(dashboard.nextReviewAt).toLocaleDateString("zh-CN", {
                    timeZone: dashboard.timeZone,
                    month: "numeric",
                    day: "numeric",
                  })
                : "学完课程或加入表达后开始复习。"}
          </span>
        </span>
      </Link>
    </div>
  );
}
