import { describe, expect, test } from "bun:test";
import { mergeEnv } from "./merge-env";

describe("mergeEnv", () => {
  test("replaces existing key with new value", () => {
    const result = mergeEnv(
      "FOO=old\nBAR=keep\n",
      new Map([["FOO", "new"]]),
    );
    expect(result).toContain("FOO=new");
    expect(result).toContain("BAR=keep");
  });

  test("preserves comments, blank lines, and unrelated vars", () => {
    const existing = "# A comment\n\nFOO=old\n# Another\nBAR=keep\n";
    const result = mergeEnv(existing, new Map([["FOO", "new"]]));
    expect(result).toContain("# A comment");
    expect(result).toContain("# Another");
    expect(result).toContain("FOO=new");
    expect(result).toContain("BAR=keep");
  });

  test("appends keys not already present", () => {
    const result = mergeEnv(
      "FOO=old\n",
      new Map([
        ["FOO", "new"],
        ["BAZ", "added"],
      ]),
    );
    expect(result).toContain("FOO=new");
    expect(result).toContain("BAZ=added");
  });

  test("handles export prefix", () => {
    const result = mergeEnv(
      "export FOO=old\nexport BAR=keep\n",
      new Map([["FOO", "new"]]),
    );
    expect(result).toContain("export FOO=new");
    expect(result).toContain("export BAR=keep");
  });

  test("idempotent: merging twice produces same result", () => {
    const vars = new Map([
      ["FOO", "new"],
      ["BAZ", "added"],
    ]);
    const first = mergeEnv("FOO=old\nBAR=keep\n", vars);
    const second = mergeEnv(first, vars);
    expect(second).toBe(first);
  });

  test("full scenario matches expected output", () => {
    const result = mergeEnv(
      "FOO=old\nBAR=keep",
      new Map([
        ["FOO", "new"],
        ["BAZ", "added"],
      ]),
    );
    expect(result).toBe("FOO=new\nBAR=keep\nBAZ=added\n");
  });

  test("empty existing string", () => {
    const result = mergeEnv(
      "",
      new Map([
        ["FOO", "bar"],
        ["BAZ", "qux"],
      ]),
    );
    expect(result).toContain("FOO=bar");
    expect(result).toContain("BAZ=qux");
  });
});
