import { Link, useNavigate } from "react-router";
import {
  SparklesIcon,
  ClockIcon,
  ExclamationTriangleIcon,
  ServerStackIcon,
  CommandLineIcon,
} from "@heroicons/react/24/outline";

const templateQueries = [
  {
    title: "Run duration by workflow",
    description: "Average execution time and run count per workflow",
    icon: ClockIcon,
    sql: "SELECT workflow_name, AVG(duration_seconds) as avg_duration,\n       COUNT(*) as run_count\nFROM runs\nGROUP BY workflow_name\nORDER BY avg_duration DESC\nLIMIT 20",
  },
  {
    title: "Daily failure rate",
    description: "Failures, totals, and failure percentage by day",
    icon: ExclamationTriangleIcon,
    sql: "SELECT date_trunc('day', created_at) as day,\n       COUNT(*) FILTER (WHERE status = 'failed') as failures,\n       COUNT(*) as total,\n       ROUND(100.0 * COUNT(*) FILTER (WHERE status = 'failed') / COUNT(*), 1) as failure_rate\nFROM runs\nGROUP BY 1\nORDER BY 1 DESC\nLIMIT 30",
  },
  {
    title: "Top repos by activity",
    description: "Run count and code churn per repository",
    icon: ServerStackIcon,
    sql: "SELECT repo, COUNT(*) as runs, SUM(additions) as total_additions,\n       SUM(deletions) as total_deletions\nFROM runs\nGROUP BY repo\nORDER BY runs DESC",
  },
];

export default function InsightsNew() {
  const navigate = useNavigate();

  return (
    <div className="mx-auto max-w-2xl py-12">
      {/* LLM input */}
      <div className="space-y-3">
        <div className="flex items-center gap-2">
          <SparklesIcon className="size-5 text-teal-500" />
          <h2 className="text-lg font-semibold text-white">New Query</h2>
        </div>
        <textarea
          placeholder="Ask assistant to generate a report"
          className="w-full rounded-lg border border-white/[0.06] bg-navy-950/80 px-4 py-3.5 text-sm text-ice-100 placeholder-navy-600 outline-none transition-colors focus:border-teal-500/40"
          rows={4}
        />
        <div className="flex justify-end">
          <button
            type="button"
            className="inline-flex items-center gap-1.5 rounded-md border border-teal-500/30 bg-teal-500/10 px-4 py-2 text-sm font-medium text-teal-300 transition-all hover:border-teal-500/50 hover:bg-teal-500/20 hover:text-white"
          >
            <SparklesIcon className="size-3.5" />
            Generate
          </button>
        </div>
      </div>

      {/* Template cards */}
      <div className="mt-10 space-y-3">
        <h3 className="text-[11px] font-semibold uppercase tracking-wider text-navy-600">
          Start from a template
        </h3>
        <div className="grid gap-3">
          {templateQueries.map((tpl) => (
            <button
              key={tpl.title}
              type="button"
              onClick={() => {
                navigate("/insights", { state: { sql: tpl.sql, name: tpl.title } });
              }}
              className="flex items-start gap-3.5 rounded-lg border border-white/[0.06] bg-navy-800/60 px-4 py-3.5 text-left transition-colors hover:border-white/[0.12] hover:bg-navy-800/80"
            >
              <tpl.icon className="mt-0.5 size-5 shrink-0 text-navy-600" />
              <div>
                <span className="text-sm font-medium text-ice-100">
                  {tpl.title}
                </span>
                <p className="mt-0.5 text-xs text-navy-600">
                  {tpl.description}
                </p>
              </div>
            </button>
          ))}
        </div>
      </div>

      {/* SQL link */}
      <div className="mt-10 text-center">
        <Link
          to="/insights"
          className="inline-flex items-center gap-1.5 text-sm text-navy-600 transition-colors hover:text-ice-300"
        >
          <CommandLineIcon className="size-4" />
          <span className="font-semibold">Write my own report with SQL</span>
        </Link>
      </div>
    </div>
  );
}
