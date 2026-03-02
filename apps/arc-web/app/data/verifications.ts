export type VerificationStatus = "pass" | "fail" | "na";

export type VerificationType = "ai" | "automated" | "analysis" | "ai-analysis";

export interface Criterion {
  name: string;
  description: string;
  type: VerificationType | null;
  status: VerificationStatus;
}

export interface VerificationCategory {
  name: string;
  question: string;
  status: VerificationStatus;
  criteria: Criterion[];
}

export const statusConfig = {
  pass: {
    label: "Pass",
    color: "text-mint",
    bg: "bg-mint/15",
    dot: "bg-mint",
    border: "border-l-mint/50",
  },
  fail: {
    label: "Fail",
    color: "text-coral",
    bg: "bg-coral/15",
    dot: "bg-coral",
    border: "border-l-coral/50",
  },
  na: {
    label: "N/A",
    color: "text-fg-muted",
    bg: "bg-overlay",
    dot: "bg-fg-muted",
    border: "border-l-fg-muted/50",
  },
} as const satisfies Record<
  VerificationStatus,
  { label: string; color: string; bg: string; dot: string; border: string }
>;

export const typeConfig = {
  ai: { label: "AI", color: "text-teal-300", bg: "bg-teal-500/10" },
  automated: { label: "Automated", color: "text-mint", bg: "bg-mint/10" },
  analysis: { label: "Analysis", color: "text-amber", bg: "bg-amber/10" },
  "ai-analysis": { label: "AI + Analysis", color: "text-teal-300", bg: "bg-teal-500/10" },
} as const satisfies Record<
  VerificationType,
  { label: string; color: string; bg: string }
>;

export const verificationCategories: VerificationCategory[] = [
  {
    name: "Traceability",
    question: "Do we understand what this change is and why we're making it?",
    status: "pass",
    criteria: [
      { name: "Motivation", description: "Origin of proposal identified", type: "ai", status: "pass" },
      { name: "Specifications", description: "Requirements written down", type: "ai", status: "pass" },
      { name: "Documentation", description: "Developer and user docs added", type: "ai", status: "pass" },
      { name: "Minimization", description: "No extraneous changes", type: "ai", status: "pass" },
    ],
  },
  {
    name: "Readability",
    question: "Can a human or agent quickly read this and understand what it does?",
    status: "pass",
    criteria: [
      { name: "Formatting", description: "Code layout matches standard", type: "automated", status: "pass" },
      { name: "Linting", description: "Linter issues resolved", type: "automated", status: "pass" },
      { name: "Style", description: "House style applied", type: "ai", status: "pass" },
    ],
  },
  {
    name: "Reliability",
    question: "Will this behave correctly and safely under real-world conditions and failures?",
    status: "pass",
    criteria: [
      { name: "Completeness", description: "Implementation covers requirements", type: "ai", status: "pass" },
      { name: "Defects", description: "Potential or likely bugs remediated", type: "ai-analysis", status: "pass" },
      { name: "Performance", description: "Hot path impact identified", type: "ai", status: "pass" },
    ],
  },
  {
    name: "Code Coverage",
    question: "Do we have trustworthy, automated evidence that it works and won't regress?",
    status: "fail",
    criteria: [
      { name: "Test Coverage", description: "Production code exercised by unit tests", type: "analysis", status: "pass" },
      { name: "Test Quality", description: "Tests are robust and clear", type: "ai", status: "fail" },
      { name: "E2E Coverage", description: "Browser automation exercises UX", type: "analysis", status: "na" },
    ],
  },
  {
    name: "Maintainability",
    question: "Will this be easy to modify or extend later without creating new risk?",
    status: "pass",
    criteria: [
      { name: "Architecture", description: "Layering and dependency graph meets design", type: "analysis", status: "pass" },
      { name: "Interfaces", description: "", type: null, status: "pass" },
      { name: "Duplication", description: "Similar and identical code blocks identified", type: "analysis", status: "pass" },
      { name: "Simplicity", description: "Extra review for reducing complexity", type: "ai", status: "pass" },
      { name: "Dead Code", description: "Unexecuted code and dependencies removed", type: "analysis", status: "pass" },
    ],
  },
  {
    name: "Security",
    question: "Does this preserve or improve our security posture and avoid vulnerabilities?",
    status: "pass",
    criteria: [
      { name: "Vulnerabilities", description: "Security issues are remediated", type: "ai-analysis", status: "pass" },
      { name: "IaC Scanning", description: "", type: null, status: "pass" },
      { name: "Dependency Alerts", description: "Known CVEs are patched", type: "analysis", status: "pass" },
      { name: "Security Controls", description: "Organization standards applied", type: "ai", status: "pass" },
    ],
  },
  {
    name: "Deployability",
    question: "Is this changeset safe to ship to production immediately?",
    status: "fail",
    criteria: [
      { name: "Compatibility", description: "Breaking changes are avoided", type: "analysis", status: "pass" },
      { name: "Rollout / Rollback", description: "Known rollback plan if deploy fails", type: "ai", status: "fail" },
      { name: "Observability", description: "Logging, metrics, tracing instrumented", type: "ai", status: "fail" },
      { name: "Cost", description: "Tech ops costs estimated", type: "analysis", status: "pass" },
    ],
  },
  {
    name: "Compliance",
    question: "Does this meet our regulatory, contractual, and policy obligations?",
    status: "pass",
    criteria: [
      { name: "Change Control", description: "Separation of Duties policy met", type: "analysis", status: "pass" },
      { name: "AI Governance", description: "AI involvement was acceptable", type: "analysis", status: "pass" },
      { name: "Privacy", description: "PII is identified and handled to standards", type: "ai", status: "pass" },
      { name: "Accessibility", description: "Software meets accessibility requirements", type: "analysis", status: "pass" },
      { name: "Licensing", description: "Supply chain meets IP policy", type: "analysis", status: "pass" },
    ],
  },
];

export type EvaluationResult = "pass" | "fail" | "skip";

export type VerificationMode = "active" | "evaluate" | "disabled";

export interface CriterionPerformance {
  f1: number | null;
  passAt1: number | null;
  mode: VerificationMode;
  evaluations: EvaluationResult[];
}

export const modeConfig = {
  active: { label: "Active", color: "text-mint", bg: "bg-mint/10" },
  evaluate: { label: "Evaluate", color: "text-amber", bg: "bg-amber/10" },
  disabled: { label: "Disabled", color: "text-fg-muted", bg: "bg-overlay" },
} as const satisfies Record<
  VerificationMode,
  { label: string; color: string; bg: string }
>;

export const criterionPerformance: Record<string, CriterionPerformance> = {
  "Motivation":          { f1: 0.87, passAt1: 0.82, mode: "active",   evaluations: ["pass","pass","fail","pass","pass","pass","pass","fail","pass","pass"] },
  "Specifications":      { f1: 0.83, passAt1: 0.78, mode: "active",   evaluations: ["pass","fail","pass","pass","pass","fail","pass","pass","pass","pass"] },
  "Documentation":       { f1: 0.79, passAt1: 0.74, mode: "active",   evaluations: ["pass","pass","pass","fail","pass","pass","fail","pass","pass","fail"] },
  "Minimization":        { f1: 0.72, passAt1: 0.68, mode: "evaluate", evaluations: ["pass","fail","pass","fail","pass","pass","fail","pass","pass","pass"] },
  "Formatting":          { f1: 0.99, passAt1: 0.98, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","pass","pass","pass","pass","pass"] },
  "Linting":             { f1: 0.98, passAt1: 0.97, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","pass","pass","pass","fail","pass"] },
  "Style":               { f1: 0.81, passAt1: 0.76, mode: "active",   evaluations: ["pass","fail","pass","pass","pass","pass","fail","pass","pass","pass"] },
  "Completeness":        { f1: 0.76, passAt1: 0.71, mode: "active",   evaluations: ["pass","pass","fail","pass","fail","pass","pass","pass","fail","pass"] },
  "Defects":             { f1: 0.84, passAt1: 0.79, mode: "active",   evaluations: ["pass","pass","pass","fail","pass","pass","pass","pass","pass","fail"] },
  "Performance":         { f1: 0.69, passAt1: 0.63, mode: "evaluate", evaluations: ["fail","pass","pass","fail","pass","fail","pass","pass","fail","pass"] },
  "Test Coverage":       { f1: 0.95, passAt1: 0.93, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","pass","fail","pass","pass","pass"] },
  "Test Quality":        { f1: 0.71, passAt1: 0.65, mode: "evaluate", evaluations: ["pass","fail","fail","pass","pass","fail","pass","fail","pass","pass"] },
  "E2E Coverage":        { f1: 0.91, passAt1: 0.88, mode: "active",   evaluations: ["pass","pass","pass","fail","pass","pass","pass","pass","pass","pass"] },
  "Architecture":        { f1: 0.88, passAt1: 0.84, mode: "active",   evaluations: ["pass","pass","pass","pass","fail","pass","pass","pass","pass","pass"] },
  "Interfaces":          { f1: null, passAt1: null, mode: "disabled",  evaluations: [] },
  "Duplication":         { f1: 0.96, passAt1: 0.94, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","pass","pass","fail","pass","pass"] },
  "Simplicity":          { f1: 0.74, passAt1: 0.69, mode: "active",   evaluations: ["pass","fail","pass","pass","fail","pass","pass","fail","pass","pass"] },
  "Dead Code":           { f1: 0.93, passAt1: 0.90, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","fail","pass","pass","pass","pass"] },
  "Vulnerabilities":     { f1: 0.86, passAt1: 0.81, mode: "active",   evaluations: ["pass","pass","fail","pass","pass","pass","pass","pass","fail","pass"] },
  "IaC Scanning":        { f1: null, passAt1: null, mode: "disabled",  evaluations: [] },
  "Dependency Alerts":   { f1: 0.97, passAt1: 0.95, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","pass","pass","pass","pass","fail"] },
  "Security Controls":   { f1: 0.80, passAt1: 0.75, mode: "active",   evaluations: ["pass","pass","fail","pass","pass","fail","pass","pass","pass","pass"] },
  "Compatibility":       { f1: 0.89, passAt1: 0.85, mode: "active",   evaluations: ["pass","pass","pass","pass","fail","pass","pass","pass","pass","pass"] },
  "Rollout / Rollback":  { f1: 0.66, passAt1: 0.60, mode: "evaluate", evaluations: ["fail","pass","fail","pass","fail","pass","pass","fail","pass","fail"] },
  "Observability":       { f1: 0.73, passAt1: 0.67, mode: "evaluate", evaluations: ["pass","fail","pass","fail","pass","pass","fail","pass","fail","pass"] },
  "Cost":                { f1: 0.78, passAt1: 0.72, mode: "evaluate", evaluations: ["pass","pass","fail","pass","fail","pass","pass","fail","pass","pass"] },
  "Change Control":      { f1: 0.94, passAt1: 0.91, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","pass","pass","pass","fail","pass"] },
  "AI Governance":       { f1: 0.85, passAt1: 0.80, mode: "active",   evaluations: ["pass","pass","pass","fail","pass","pass","pass","pass","pass","pass"] },
  "Privacy":             { f1: 0.77, passAt1: 0.72, mode: "active",   evaluations: ["pass","fail","pass","pass","pass","fail","pass","pass","pass","fail"] },
  "Accessibility":       { f1: 0.90, passAt1: 0.87, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","fail","pass","pass","pass","pass"] },
  "Licensing":           { f1: 0.96, passAt1: 0.93, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","pass","pass","pass","pass","pass"] },
};

export function getCategorySummary(categories: readonly VerificationCategory[]) {
  const passing = categories.filter((c) => c.status === "pass").length;
  return { passing, total: categories.length };
}

export function getCriteriaSummary(criteria: readonly Criterion[]) {
  return {
    passing: criteria.filter((c) => c.status === "pass").length,
    failing: criteria.filter((c) => c.status === "fail").length,
    na: criteria.filter((c) => c.status === "na").length,
    total: criteria.length,
  };
}

export function getAllCriteria(categories: readonly VerificationCategory[]) {
  return categories.flatMap((c) => c.criteria);
}

export function slugify(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/(^-|-$)/g, "");
}

export function findCriterionBySlug(slug: string): {
  criterion: Criterion;
  category: VerificationCategory;
  performance: CriterionPerformance;
} | null {
  for (const category of verificationCategories) {
    for (const criterion of category.criteria) {
      if (slugify(criterion.name) === slug) {
        const performance = criterionPerformance[criterion.name];
        if (performance) {
          return { criterion, category, performance };
        }
      }
    }
  }
  return null;
}

export interface ControlDetail {
  description: string;
  checks: string[];
  passExample: string;
  failExample: string;
}

export const controlDetails: Record<string, ControlDetail> = {
  "Motivation": {
    description: "Verifies that every change traces back to a clear origin — whether a ticket, RFC, customer request, or incident. Without documented motivation, reviewers lack context for evaluating whether the change is appropriate.",
    checks: ["PR body or linked issue explains why the change is needed", "Commit messages reference a ticket or context", "No orphaned changes without traceable origin"],
    passExample: "PR links to JIRA-1234 and explains the user-facing pain point being resolved.",
    failExample: "PR description is empty or says only 'fix stuff'.",
  },
  "Specifications": {
    description: "Checks that functional and non-functional requirements are written down before implementation begins. Specifications prevent scope creep and ensure everyone agrees on what done looks like.",
    checks: ["Acceptance criteria listed in the issue or PR", "Edge cases documented", "Non-functional requirements (performance, security) stated when relevant"],
    passExample: "Issue includes acceptance criteria with three testable scenarios.",
    failExample: "Issue body says 'implement the feature' with no acceptance criteria.",
  },
  "Documentation": {
    description: "Ensures developer-facing and user-facing documentation is added or updated alongside code changes. Stale docs degrade team velocity and increase onboarding cost.",
    checks: ["README or docs updated for new features", "API documentation reflects endpoint changes", "Inline comments for non-obvious logic"],
    passExample: "New API endpoint has corresponding OpenAPI spec update and usage example in docs.",
    failExample: "New CLI flag added with no mention in README or --help text.",
  },
  "Minimization": {
    description: "Flags extraneous changes that inflate the diff — formatting-only edits, unrelated refactors, or drive-by fixes. Keeping PRs focused improves review quality and reduces revert risk.",
    checks: ["No unrelated formatting or whitespace changes", "Refactors separated from feature work", "Each commit addresses a single concern"],
    passExample: "PR touches only files directly related to the new caching layer.",
    failExample: "PR adds a feature but also reformats 12 unrelated files.",
  },
  "Formatting": {
    description: "Validates that code layout conforms to the project's formatting standard (e.g., Prettier, rustfmt). Automated formatting removes subjective style debates from code review.",
    checks: ["All files pass the project formatter", "No manual formatting overrides without justification"],
    passExample: "All changed files pass `prettier --check` and `rustfmt --check`.",
    failExample: "Several files have inconsistent indentation that the formatter would fix.",
  },
  "Linting": {
    description: "Confirms that static analysis findings are resolved. Linter warnings left unaddressed accumulate into tech debt and mask real issues.",
    checks: ["No new linter warnings introduced", "Existing warnings not suppressed without explanation", "Lint config not weakened"],
    passExample: "ESLint and Clippy pass with zero warnings on changed files.",
    failExample: "New `// eslint-disable-next-line` added to suppress a legitimate warning.",
  },
  "Style": {
    description: "Evaluates whether the code follows the team's house style conventions beyond what automated formatters catch — naming, file organization, import ordering, and idiomatic patterns.",
    checks: ["Naming conventions followed (camelCase, snake_case as appropriate)", "Import ordering matches project convention", "Idiomatic patterns used for the language"],
    passExample: "New TypeScript module uses camelCase variables, groups imports by source, and uses `Map` instead of plain objects for lookups.",
    failExample: "Mix of camelCase and snake_case in the same module with random import ordering.",
  },
  "Completeness": {
    description: "Checks that the implementation fully covers the specified requirements. Partial implementations ship broken experiences and create follow-up tickets that could have been avoided.",
    checks: ["All acceptance criteria addressed", "Edge cases handled", "Error states implemented"],
    passExample: "Feature handles all three specified user roles with appropriate permissions.",
    failExample: "Only the happy path is implemented; error and empty states are missing.",
  },
  "Defects": {
    description: "Identifies potential or likely bugs through static analysis and AI review. Catching defects before merge is orders of magnitude cheaper than finding them in production.",
    checks: ["No off-by-one errors in loops or slices", "Null/undefined handled at boundaries", "Race conditions considered in async code"],
    passExample: "API handler validates input, handles missing fields gracefully, and returns appropriate HTTP status codes.",
    failExample: "Array index accessed without bounds check; crashes on empty input.",
  },
  "Performance": {
    description: "Assesses whether the change impacts hot paths or introduces algorithmic regressions. Performance problems that ship to production are expensive to diagnose and fix.",
    checks: ["No N+1 queries introduced", "Large collections not processed synchronously", "Caching considered for repeated expensive operations"],
    passExample: "Database query uses a JOIN instead of N separate queries for related records.",
    failExample: "Loop makes a separate HTTP call for each item in a 1000-element list.",
  },
  "Test Coverage": {
    description: "Measures whether production code is exercised by automated tests. Coverage gaps mean regressions can ship undetected.",
    checks: ["New code has corresponding unit tests", "Coverage does not decrease", "Critical paths have integration tests"],
    passExample: "New service method has 6 unit tests covering happy path, error cases, and edge cases.",
    failExample: "New 200-line module has zero test files.",
  },
  "Test Quality": {
    description: "Evaluates whether tests are robust, readable, and actually verify behavior rather than implementation details. Low-quality tests give false confidence.",
    checks: ["Tests verify behavior, not implementation", "Assertions are specific and meaningful", "Tests are independent and deterministic"],
    passExample: "Tests assert on API response shape and status codes, not on internal method call counts.",
    failExample: "Tests mock every dependency and only verify that mocks were called.",
  },
  "E2E Coverage": {
    description: "Checks that user-facing workflows are exercised by end-to-end browser automation. E2E tests catch integration issues that unit tests miss.",
    checks: ["Critical user flows have Playwright/Cypress tests", "E2E tests run in CI", "No flaky E2E tests introduced"],
    passExample: "New checkout flow has a Playwright test that completes a purchase end-to-end.",
    failExample: "New multi-step wizard has no browser automation tests.",
  },
  "Architecture": {
    description: "Validates that layering and dependency directions conform to the project's architectural design. Architectural violations compound over time and make systems harder to evolve.",
    checks: ["Dependencies point inward (domain doesn't depend on infra)", "No circular dependencies introduced", "Module boundaries respected"],
    passExample: "New repository implementation depends on domain interfaces, not the other way around.",
    failExample: "Domain model imports directly from the HTTP framework package.",
  },
  "Interfaces": {
    description: "Reviews public API surfaces for clarity, consistency, and backward compatibility. Interfaces are contracts — once published, they're expensive to change.",
    checks: ["Public API types are well-defined", "Breaking changes documented", "Consistent naming across endpoints"],
    passExample: "New endpoint follows existing naming and error format conventions.",
    failExample: "New endpoint uses different error format than all other endpoints.",
  },
  "Duplication": {
    description: "Detects similar or identical code blocks that could be consolidated. Duplication increases maintenance burden and creates inconsistency risk.",
    checks: ["No copy-pasted logic across files", "Shared utilities used for common patterns", "Similar test setup consolidated"],
    passExample: "Date formatting logic extracted into a shared utility used by 4 components.",
    failExample: "Same 15-line validation function copy-pasted into three different handlers.",
  },
  "Simplicity": {
    description: "Flags unnecessarily complex code that could be simplified without changing behavior. Simpler code is easier to review, debug, and extend.",
    checks: ["No premature abstractions", "Control flow is straightforward", "Functions are focused and short"],
    passExample: "Conditional logic uses early returns instead of deeply nested if-else chains.",
    failExample: "Three-level generic abstraction for a function called in one place.",
  },
  "Dead Code": {
    description: "Identifies unexecuted code paths and unused dependencies. Dead code misleads readers and bloats bundles.",
    checks: ["No unreachable code paths", "Unused imports and variables removed", "Deprecated functions removed if no longer called"],
    passExample: "Old feature flag and its associated code paths removed after rollout completed.",
    failExample: "Commented-out function left in file 'in case we need it later'.",
  },
  "Vulnerabilities": {
    description: "Scans for known security vulnerabilities using both AI analysis and static scanning tools. Shipping known vulnerabilities exposes users and the organization to risk.",
    checks: ["No SQL injection or XSS vectors", "User input sanitized at boundaries", "Authentication/authorization checks present"],
    passExample: "User input passed through parameterized queries; HTML output escaped.",
    failExample: "Raw SQL string concatenation with user-supplied values.",
  },
  "IaC Scanning": {
    description: "Validates infrastructure-as-code definitions against security best practices. Misconfigured infrastructure is a leading cause of data breaches.",
    checks: ["No publicly accessible storage buckets", "Encryption at rest enabled", "Least-privilege IAM policies"],
    passExample: "Terraform module creates S3 bucket with encryption, versioning, and private ACL.",
    failExample: "CloudFormation template creates an RDS instance with no encryption and public accessibility.",
  },
  "Dependency Alerts": {
    description: "Checks that third-party dependencies are free from known CVEs. Vulnerable dependencies are an easy attack vector that automated tools can detect.",
    checks: ["No dependencies with known critical CVEs", "Lock file updated to patched versions", "Unused dependencies removed"],
    passExample: "Dependabot alert resolved by updating lodash from 4.17.20 to 4.17.21.",
    failExample: "Package.json pins a version of axios with a known SSRF vulnerability.",
  },
  "Security Controls": {
    description: "Verifies that organization-specific security standards are applied — rate limiting, audit logging, CORS policies, and secret management.",
    checks: ["Secrets not hardcoded in source", "Rate limiting on public endpoints", "Audit logging for sensitive operations"],
    passExample: "API key loaded from environment variable; rate limiter configured on login endpoint.",
    failExample: "AWS credentials committed in a config file.",
  },
  "Compatibility": {
    description: "Detects breaking changes in APIs, database schemas, or wire formats that could disrupt consumers. Breaking changes require coordination that surprises prevent.",
    checks: ["No removed or renamed public API fields", "Database migrations are backward-compatible", "Wire format changes are additive"],
    passExample: "New field added to API response; no existing fields removed or renamed.",
    failExample: "Column renamed in migration while old code is still deployed.",
  },
  "Rollout / Rollback": {
    description: "Confirms that the change has a clear deployment plan and can be safely rolled back if issues arise. Every production deploy should be reversible.",
    checks: ["Feature flag available for gradual rollout", "Database migration is reversible", "Rollback procedure documented"],
    passExample: "Feature behind a LaunchDarkly flag with 10% initial rollout and documented rollback steps.",
    failExample: "Irreversible database migration with no rollback plan.",
  },
  "Observability": {
    description: "Ensures that logging, metrics, and tracing are instrumented for new code paths. Without observability, production issues are invisible until users report them.",
    checks: ["Structured logging for new operations", "Metrics emitted for key business events", "Distributed tracing propagated"],
    passExample: "New payment endpoint logs transaction IDs, emits latency metrics, and propagates trace context.",
    failExample: "New background job has no logging or metrics; failures are silent.",
  },
  "Cost": {
    description: "Estimates the infrastructure and operational cost impact of the change. Unchecked cost growth erodes margins and can cause budget surprises.",
    checks: ["New infrastructure resources sized appropriately", "No unbounded resource consumption", "Cost estimate provided for significant changes"],
    passExample: "New Lambda function has memory limit set and estimated monthly cost noted in PR.",
    failExample: "New service provisions a db.r5.4xlarge for a table with 100 rows.",
  },
  "Change Control": {
    description: "Validates that separation-of-duties policies are met — the author is not the sole reviewer, approvals are obtained, and the change went through the proper process.",
    checks: ["PR has at least one approval from non-author", "Required reviewers have signed off", "No self-merging without policy exception"],
    passExample: "PR approved by two team members before merge; CI checks all green.",
    failExample: "Author approved and merged their own PR with no other reviewers.",
  },
  "AI Governance": {
    description: "Checks that AI-generated or AI-assisted code meets the organization's governance requirements — attribution, review depth, and acceptable use.",
    checks: ["AI-generated code clearly attributed", "Human review of AI suggestions documented", "AI usage within acceptable-use policy"],
    passExample: "PR notes that implementation was AI-assisted; human reviewer verified logic and tests.",
    failExample: "Entire module generated by AI with no human review or attribution.",
  },
  "Privacy": {
    description: "Ensures that personally identifiable information (PII) is identified, classified, and handled according to privacy standards (GDPR, CCPA).",
    checks: ["PII fields identified and documented", "Data retention policies applied", "Consent mechanisms in place for data collection"],
    passExample: "New user profile endpoint masks email in logs and respects data deletion requests.",
    failExample: "User email addresses logged in plaintext to application logs.",
  },
  "Accessibility": {
    description: "Verifies that UI changes meet accessibility requirements (WCAG 2.1 AA). Inaccessible software excludes users and creates legal risk.",
    checks: ["Semantic HTML elements used", "ARIA labels present on interactive elements", "Color contrast meets WCAG AA standards"],
    passExample: "New modal uses <dialog>, has aria-labelledby, and focus is trapped within.",
    failExample: "Custom dropdown built with <div> elements, no keyboard navigation, no ARIA roles.",
  },
  "Licensing": {
    description: "Ensures that all third-party dependencies comply with the organization's intellectual property policy. License violations can have severe legal consequences.",
    checks: ["No GPL-licensed dependencies in proprietary code", "License file present for new dependencies", "Supply chain attestation where required"],
    passExample: "New dependency uses MIT license; added to approved dependency list.",
    failExample: "AGPL-licensed library added to a closed-source commercial product.",
  },
};

export interface RecentControlResult {
  runId: string;
  runTitle: string;
  workflow: string;
  result: VerificationStatus;
  timestamp: string;
}

export const recentControlResults: Record<string, RecentControlResult[]> = {
  "Motivation": [
    { runId: "run-047", runTitle: "PR #312 — Add OAuth2 PKCE flow", workflow: "code_review", result: "pass", timestamp: "2h ago" },
    { runId: "run-046", runTitle: "PR #311 — Update rate limiter config", workflow: "code_review", result: "pass", timestamp: "5h ago" },
    { runId: "run-044", runTitle: "PR #309 — Migrate to pnpm", workflow: "code_review", result: "fail", timestamp: "1d ago" },
    { runId: "run-042", runTitle: "PR #307 — Fix session timeout", workflow: "fix_build", result: "pass", timestamp: "2d ago" },
    { runId: "run-040", runTitle: "PR #305 — Add webhook retries", workflow: "code_review", result: "pass", timestamp: "3d ago" },
  ],
  "Documentation": [
    { runId: "run-047", runTitle: "PR #312 — Add OAuth2 PKCE flow", workflow: "code_review", result: "pass", timestamp: "2h ago" },
    { runId: "run-046", runTitle: "PR #311 — Update rate limiter config", workflow: "code_review", result: "fail", timestamp: "5h ago" },
    { runId: "run-044", runTitle: "PR #309 — Migrate to pnpm", workflow: "code_review", result: "pass", timestamp: "1d ago" },
    { runId: "run-042", runTitle: "PR #307 — Fix session timeout", workflow: "fix_build", result: "pass", timestamp: "2d ago" },
    { runId: "run-040", runTitle: "PR #305 — Add webhook retries", workflow: "code_review", result: "fail", timestamp: "3d ago" },
  ],
  "Rollout / Rollback": [
    { runId: "run-047", runTitle: "PR #312 — Add OAuth2 PKCE flow", workflow: "code_review", result: "fail", timestamp: "2h ago" },
    { runId: "run-046", runTitle: "PR #311 — Update rate limiter config", workflow: "code_review", result: "pass", timestamp: "5h ago" },
    { runId: "run-044", runTitle: "PR #309 — Migrate to pnpm", workflow: "code_review", result: "fail", timestamp: "1d ago" },
    { runId: "run-042", runTitle: "PR #307 — Fix session timeout", workflow: "fix_build", result: "fail", timestamp: "2d ago" },
    { runId: "run-040", runTitle: "PR #305 — Add webhook retries", workflow: "code_review", result: "pass", timestamp: "3d ago" },
  ],
};

// Default recent results for controls without specific data
const defaultRecentResults: RecentControlResult[] = [
  { runId: "run-047", runTitle: "PR #312 — Add OAuth2 PKCE flow", workflow: "code_review", result: "pass", timestamp: "2h ago" },
  { runId: "run-046", runTitle: "PR #311 — Update rate limiter config", workflow: "code_review", result: "pass", timestamp: "5h ago" },
  { runId: "run-044", runTitle: "PR #309 — Migrate to pnpm", workflow: "code_review", result: "pass", timestamp: "1d ago" },
  { runId: "run-042", runTitle: "PR #307 — Fix session timeout", workflow: "fix_build", result: "pass", timestamp: "2d ago" },
  { runId: "run-040", runTitle: "PR #305 — Add webhook retries", workflow: "code_review", result: "pass", timestamp: "3d ago" },
];

export function getRecentResults(criterionName: string): RecentControlResult[] {
  return recentControlResults[criterionName] ?? defaultRecentResults;
}
