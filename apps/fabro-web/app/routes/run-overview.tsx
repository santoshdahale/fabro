import { useState, useEffect, useRef } from "react";
import { Link, useParams, useRevalidator } from "react-router";
import { CheckCircleIcon, ArrowPathIcon, PauseCircleIcon, XCircleIcon } from "@heroicons/react/24/solid";
import { DocumentTextIcon, MapIcon } from "@heroicons/react/24/outline";
import { apiFetch, apiJsonOrNull } from "../api";
import { isVisibleStage } from "../data/runs";
import { formatDurationSecs } from "../lib/format";
import type { PaginatedRunStageList } from "@qltysh/fabro-api-client";

export const handle = { wide: true };

type StageStatus = "completed" | "running" | "pending" | "failed" | "cancelled";

interface Stage {
  id: string;
  name: string;
  status: StageStatus;
  duration: string;
}

export async function loader({ request, params }: any) {
  const stagesResult = await apiJsonOrNull<PaginatedRunStageList>(
    `/runs/${params.id}/stages`,
    { request },
  );
  const stages: Stage[] = (stagesResult?.data ?? []).filter((s) => isVisibleStage(s.id)).map((s) => ({
    id: s.id,
    name: s.name,
    status: s.status as StageStatus,
    duration: s.duration_secs != null ? formatDurationSecs(s.duration_secs) : "--",
  }));
  const graphRes = await apiFetch(`/runs/${params.id}/graph`, { request });
  const graphSvg = graphRes.ok ? await graphRes.text() : null;
  return { stages, graphSvg };
}

const statusConfig: Record<StageStatus, { icon: typeof CheckCircleIcon; color: string }> = {
  completed: { icon: CheckCircleIcon, color: "text-mint" },
  running: { icon: ArrowPathIcon, color: "text-teal-500" },
  pending: { icon: PauseCircleIcon, color: "text-fg-muted" },
  failed: { icon: XCircleIcon, color: "text-coral" },
  cancelled: { icon: XCircleIcon, color: "text-fg-muted" },
};

const STAGE_EVENTS = new Set([
  "stage.started", "stage.completed", "stage.failed",
  "run.completed", "run.failed",
]);

export default function RunOverview({ loaderData }: any) {
  const { id } = useParams();
  const { stages, graphSvg } = loaderData;
  const revalidator = useRevalidator();

  // Track when we first observed each running stage (for ticking timer)
  const runningStartRef = useRef<Map<string, number>>(new Map());
  const [, setTick] = useState(0);

  // Subscribe to SSE for live stage updates
  useEffect(() => {
    const source = new EventSource("/api/v1/attach");
    let debounceTimer: ReturnType<typeof setTimeout> | undefined;

    source.onmessage = (msg) => {
      try {
        const payload = JSON.parse(msg.data);
        if (payload.run_id === id && STAGE_EVENTS.has(payload.event)) {
          clearTimeout(debounceTimer);
          debounceTimer = setTimeout(() => revalidator.revalidate(), 300);
        }
      } catch {
        // ignore malformed events
      }
    };

    return () => {
      clearTimeout(debounceTimer);
      source.close();
    };
  }, [id]);

  // Track start times for running stages
  useEffect(() => {
    const running = new Set<string>(
      stages.filter((s: Stage) => s.status === "running").map((s: Stage) => s.id),
    );
    for (const stageId of running) {
      if (!runningStartRef.current.has(stageId)) {
        runningStartRef.current.set(stageId, Date.now());
      }
    }
    for (const stageId of runningStartRef.current.keys()) {
      if (!running.has(stageId)) {
        runningStartRef.current.delete(stageId);
      }
    }
  }, [stages]);

  // Tick every second while any stage is running
  useEffect(() => {
    if (!stages.some((s: Stage) => s.status === "running")) return;
    const interval = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(interval);
  }, [stages]);

  function stageDuration(stage: Stage): string {
    if (stage.status === "running") {
      const start = runningStartRef.current.get(stage.id);
      if (start) return formatDurationSecs(Math.floor((Date.now() - start) / 1000));
      return "0s";
    }
    return stage.duration;
  }

  return (
    <div className="flex gap-6">
      <nav className="w-56 shrink-0 space-y-6">
        {stages.length > 0 && (
          <div>
            <h3 className="px-2 text-xs font-medium uppercase tracking-wider text-fg-muted">Stages</h3>
            <ul className="mt-2 space-y-0.5">
              {stages.map((stage: Stage) => {
                const config = statusConfig[stage.status];
                const Icon = config.icon;
                return (
                  <li key={stage.id}>
                    <Link
                      to={`/runs/${id}/stages/${stage.id}`}
                      className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-fg-3 transition-colors hover:bg-overlay hover:text-fg"
                    >
                      <Icon className={`size-4 shrink-0 ${config.color} ${stage.status === "running" ? "animate-spin" : ""}`} />
                      <span className="flex-1 truncate">{stage.name}</span>
                      <span className="font-mono text-xs tabular-nums text-fg-muted">{stageDuration(stage)}</span>
                    </Link>
                  </li>
                );
              })}
            </ul>
          </div>
        )}

        <div>
          <h3 className="px-2 text-xs font-medium uppercase tracking-wider text-fg-muted">Workflow</h3>
          <ul className="mt-2 space-y-0.5">
            <li>
              <Link
                to={`/runs/${id}/settings`}
                className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-fg-3 transition-colors hover:bg-overlay hover:text-fg"
              >
                <DocumentTextIcon className="size-4 shrink-0 text-fg-muted" />
                Run Settings
              </Link>
            </li>
            <li>
              <Link
                to={`/runs/${id}/graph`}
                className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-fg-3 transition-colors hover:bg-overlay hover:text-fg"
              >
                <MapIcon className="size-4 shrink-0 text-fg-muted" />
                Workflow Graph
              </Link>
            </li>
          </ul>
        </div>
      </nav>

      <div className="min-w-0 flex-1">
        {graphSvg ? (
          <div
            className="graph-svg rounded-md border border-line bg-panel-alt/40 overflow-auto p-6 [&_svg]:mx-auto [&_svg]:block"
            dangerouslySetInnerHTML={{ __html: graphSvg }}
          />
        ) : (
          <p className="text-sm text-fg-muted">No workflow graph available.</p>
        )}
      </div>
    </div>
  );
}
