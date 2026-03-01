import { Link, Outlet, useNavigate } from "react-router";
import { PlusIcon } from "@heroicons/react/24/outline";
import type { Route } from "./+types/insights";

export function meta({}: Route.MetaArgs) {
  return [{ title: "Insights — Arc" }];
}

export const handle = {
  wide: true,
};

// ── Types ──

export interface SavedQuery {
  id: string;
  name: string;
  sql: string;
}

export interface HistoryEntry {
  id: string;
  sql: string;
  timestamp: string;
  elapsed: number;
  rowsReturned: number;
}

// ── Mock data ──

export const savedQueries: SavedQuery[] = [
  {
    id: "1",
    name: "Run duration by workflow",
    sql: "SELECT workflow_name, AVG(duration_seconds) as avg_duration,\n       COUNT(*) as run_count\nFROM runs\nGROUP BY workflow_name\nORDER BY avg_duration DESC\nLIMIT 20",
  },
  {
    id: "2",
    name: "Daily failure rate",
    sql: "SELECT date_trunc('day', created_at) as day,\n       COUNT(*) FILTER (WHERE status = 'failed') as failures,\n       COUNT(*) as total,\n       ROUND(100.0 * COUNT(*) FILTER (WHERE status = 'failed') / COUNT(*), 1) as failure_rate\nFROM runs\nGROUP BY 1\nORDER BY 1 DESC\nLIMIT 30",
  },
  {
    id: "3",
    name: "Top repos by activity",
    sql: "SELECT repo, COUNT(*) as runs, SUM(additions) as total_additions,\n       SUM(deletions) as total_deletions\nFROM runs\nGROUP BY repo\nORDER BY runs DESC",
  },
];

export const historyEntries: HistoryEntry[] = [
  { id: "h1", sql: "SELECT workflow_name, COUNT(*) FROM runs GROUP BY 1", timestamp: "2 min ago", elapsed: 0.342, rowsReturned: 6 },
  { id: "h2", sql: "SELECT * FROM runs WHERE status = 'failed' LIMIT 100", timestamp: "8 min ago", elapsed: 0.127, rowsReturned: 23 },
  { id: "h3", sql: "SELECT date_trunc('day', created_at) as d, COUNT(*) FROM runs GROUP BY 1 ORDER BY 1", timestamp: "15 min ago", elapsed: 0.531, rowsReturned: 30 },
  { id: "h4", sql: "SELECT repo, AVG(duration_seconds) FROM runs GROUP BY repo", timestamp: "1 hr ago", elapsed: 0.089, rowsReturned: 12 },
  { id: "h5", sql: "DESCRIBE runs", timestamp: "1 hr ago", elapsed: 0.003, rowsReturned: 18 },
];

export default function InsightsLayout() {
  const navigate = useNavigate();

  return (
    <div className="flex gap-6">
      {/* ── Sidebar ── */}
      <div className="w-56 shrink-0">
        <div className="sticky top-6 space-y-4">
          <Link
            to="/insights/new"
            className="inline-flex w-full items-center justify-center gap-1.5 rounded-md border border-white/[0.06] bg-navy-800/80 px-3 py-2 text-sm font-medium text-ice-300 transition-colors hover:border-white/[0.12] hover:bg-navy-800 hover:text-white"
          >
            <PlusIcon className="size-3.5" />
            New Query
          </Link>

          <div>
            <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-navy-600">
              Saved Queries
            </h3>
            <div className="space-y-0.5">
              {savedQueries.map((q) => (
                <button
                  key={q.id}
                  type="button"
                  onClick={() => {
                    navigate("/insights", { state: { sql: q.sql, name: q.name } });
                  }}
                  className="flex w-full flex-col gap-0.5 rounded-md px-2.5 py-2 text-left transition-colors hover:bg-white/[0.05]"
                >
                  <span className="text-sm font-medium text-ice-100">
                    {q.name}
                  </span>
                  <span className="truncate font-mono text-[10px] text-navy-600">
                    {q.sql.split("\n")[0]}
                  </span>
                </button>
              ))}
            </div>
          </div>

          <div>
            <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-navy-600">
              History
            </h3>
            <div className="space-y-0.5">
              {historyEntries.map((entry) => (
                <button
                  key={entry.id}
                  type="button"
                  onClick={() => {
                    navigate("/insights", { state: { sql: entry.sql } });
                  }}
                  className="flex w-full flex-col gap-0.5 rounded-md px-2.5 py-2 text-left transition-colors hover:bg-white/[0.05]"
                >
                  <span className="truncate font-mono text-[10px] text-ice-300">
                    {entry.sql}
                  </span>
                  <span className="font-mono text-[10px] text-navy-600">
                    {entry.timestamp} · {entry.rowsReturned} rows
                  </span>
                </button>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* ── Main content ── */}
      <div className="min-w-0 flex-1">
        <Outlet />
      </div>
    </div>
  );
}
