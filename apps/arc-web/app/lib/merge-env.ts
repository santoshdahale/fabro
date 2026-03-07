/**
 * Merge new key=value pairs into an existing .env file string.
 * - Replaces lines whose key matches (handles optional `export ` prefix)
 * - Preserves comments, blank lines, and unrelated variables
 * - Appends keys not already present
 */
export function mergeEnv(
  existing: string,
  newVars: Map<string, string>,
): string {
  const handledKeys = new Set<string>();
  const resultLines: string[] = [];

  for (const line of existing.split("\n")) {
    const eqPos = line.indexOf("=");
    if (eqPos !== -1) {
      let rawKey = line.slice(0, eqPos).trim();
      const hasExport = rawKey.startsWith("export ");
      if (hasExport) {
        rawKey = rawKey.slice("export ".length).trim();
      }
      if (rawKey.length > 0 && !rawKey.startsWith("#")) {
        const newVal = newVars.get(rawKey);
        if (newVal !== undefined) {
          const prefix = hasExport ? "export " : "";
          resultLines.push(`${prefix}${rawKey}=${newVal}`);
          handledKeys.add(rawKey);
          continue;
        }
      }
    }
    resultLines.push(line);
  }

  for (const [key, val] of newVars) {
    if (!handledKeys.has(key)) {
      resultLines.push(`${key}=${val}`);
    }
  }

  let result = resultLines.join("\n");
  if (!result.endsWith("\n")) {
    result += "\n";
  }
  return result;
}
