import { useParams } from "react-router";
import { apiFetch, apiJsonOrNull } from "../api";
import { isVisibleStage } from "../data/runs";
import { formatDurationSecs } from "../lib/format";
import { StageSidebar } from "../components/stage-sidebar";
import type { Stage } from "../components/stage-sidebar";
import type { PaginatedRunStageList } from "@qltysh/fabro-api-client";

export const handle = { wide: true };

export async function loader({ request, params }: any) {
  const stagesResult = await apiJsonOrNull<PaginatedRunStageList>(
    `/runs/${params.id}/stages`,
    { request },
  );
  const stages: Stage[] = (stagesResult?.data ?? []).filter((s) => isVisibleStage(s.id)).map((s) => ({
    id: s.id,
    name: s.name,
    status: s.status as Stage["status"],
    duration: s.duration_secs != null ? formatDurationSecs(s.duration_secs) : "--",
  }));
  const graphRes = await apiFetch(`/runs/${params.id}/graph`, { request });
  const graphSvg = graphRes.ok ? await graphRes.text() : null;
  return { stages, graphSvg };
}

export default function RunOverview({ loaderData }: any) {
  const { id } = useParams();
  const { stages, graphSvg } = loaderData;

  return (
    <div className="flex gap-6">
      <StageSidebar stages={stages} runId={id!} />

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
