import type { RunCommit } from "@qltysh/fabro-api-client";

export type RunCommitPickerOption = {
  sha: string;
  label: string;
  title: string;
  fromSha: string | null;
  toSha: string;
};

export function fabroGeneratedCommitStage(subject: string): string | null {
  const match = subject.match(/^fabro\([^)]+\):\s+(.+?)\s+\([^)]+\)$/);
  const stage = match?.[1]?.trim();
  return stage ? stage : null;
}

export function buildRunCommitOptions(
  commits: Pick<RunCommit, "sha" | "short_sha" | "subject" | "parents">[],
): RunCommitPickerOption[] {
  const generatedVisits = new Map<string, number>();
  return commits.map((commit) => {
    const stage = fabroGeneratedCommitStage(commit.subject);
    let label = commit.subject || commit.short_sha;
    if (stage) {
      const visit = (generatedVisits.get(stage) ?? 0) + 1;
      generatedVisits.set(stage, visit);
      label = `${stage}@${visit}`;
    }
    return {
      sha:     commit.sha,
      fromSha: commit.parents[0]?.sha ?? null,
      toSha:   commit.sha,
      label,
      title:   `${commit.short_sha} ${commit.subject}`.trim(),
    };
  });
}
