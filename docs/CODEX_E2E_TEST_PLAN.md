# ZhiForge Codex E2E Test Plan

> Purpose: finish the tests that require real external credentials or network services after the no-API productization QA has passed.
>
> Scope: AI provider calls, Zhihu Developer API search/reader flow, AI internalization, grounded question generation/judgement, semantic reranking, AI review planning, and AI Librarian flows.

## 0. Safety rules

1. Do **not** print, echo, log, commit, paste, or return any API key / Access Secret.
2. Use credentials already configured by the user in ZhiForge / Windows Credential Manager. If a required credential is absent, report which credential class is missing and stop only that test group.
3. Do not overwrite the user's existing provider credentials merely to test an error path.
4. Before tests that write knowledge data, create a manual database backup from ZhiForge's `数据与备份` page and record only the backup filename.
5. Do not permanently delete user data. Test-created Knowledge Units may be moved to Trash after verification; do not empty Trash.
6. Keep a list of every Source ID / Knowledge Unit ID / Review Session ID created by this run.
7. Treat `preview/` as unrelated user material. Do not modify or remove it.
8. Never push, tag, publish, or create a GitHub Release as part of this test plan unless the user explicitly asks.

## 1. Baseline already verified

Do not waste time rerunning these unless an API/E2E failure suggests a regression:

- Windows single-instance behavior.
- Windows Credential Manager self-test.
- SQLite schema v12 and `PRAGMA quick_check = ok`.
- automatic backup 24h deduplication.
- isolated 10k Knowledge / 2k Source / 20k Relation stress tests.
- Library full-database search performance fix for correlated FTS.
- Review Queue, Graph, Source Library and Knowledge Health large-database query performance.
- online SQLite backup consistency.
- readable export performance at 1k and 10k Knowledge Units.
- full Rust suite: 122 passed / 0 failed / 1 ignored.
- `pnpm check` and production frontend build.
- optimized Windows Release build and Release EXE startup smoke.

## 2. Environment discovery

Record without exposing secrets:

- git HEAD and working-tree status;
- app version;
- configured AI provider names and enabled routes, but not keys or Authorization headers;
- whether a Zhihu Access Secret is configured (`true/false` only);
- database path and schema version;
- counts of Sources, Knowledge Units, Questions, Attempts, Review Scheduler Events and AI Jobs.

If the app is already running, reuse it where practical. Avoid many concurrent Tauri processes.

## 3. AI provider connectivity smoke

### Goal
Prove that the configured production AI route can complete one real request end-to-end.

### Procedure

1. Read the enabled primary route and fallback route(s) without exposing keys.
2. Use an existing low-cost action such as translation/polish or the smallest routed request supported by the app.
3. Use deterministic short input: `ZhiForge keeps source evidence separate from generated knowledge.`
4. Verify response is non-empty, no raw secret appears in logs, and normal timeout/retry logic is not spuriously triggered.
5. If the primary route naturally fails before output and fallback is configured, verify fallback once. Do not intentionally corrupt credentials.

### Pass
At least one configured real AI route returns a valid response through ZhiForge's production execution path.

## 4. Zhihu Developer API search and Reader E2E

### Goal
Verify the real Zhihu Access Secret, search API, result parsing, sorting and Reader open flow.

### Procedure

1. Confirm only that a Zhihu Access Secret is configured.
2. Search a benign query with multiple likely results, e.g. `学习方法`.
3. Historical testing found rapid repeated requests can return `Code=30001`; use serial requests and avoid bursts. Leave several seconds before a retry.
4. Verify HTTP succeeds, at least one response has `Code=0`, results deserialize, title/author/url/content identifiers are usable, sort modes do not corrupt the result set, Reader opens one result, and `打开知乎` targets the expected URL.
5. Record query, result count, latency and selected result title. Never record the Access Secret or Authorization header.

### Pass
A real search returns usable results and at least one result opens in Reader.

## 5. Full-answer internalization: one Source -> multiple Knowledge Units

### Goal
Verify the v11 Source cardinality change and current AI internalization contract with a real model call.

Choose one Reader answer containing at least two distinct, independently useful claims.

1. Use `从全文内化`.
2. Wait for the internalization job to complete.
3. Capture Source ID and all generated Knowledge Unit IDs.
4. Verify:
   - one Source exists for the answer;
   - 1–4 KUs are produced;
   - for clearly multi-claim text, prefer >=2 KUs; if the model reasonably emits 1, repeat once with a stronger multi-claim source before failing;
   - every KU points to the same Source;
   - each KU has a distinct non-empty `core_claim`;
   - each KU has at least one Evidence exact-contiguous quote from Source text;
   - each KU has a primary Claim linked to supporting Evidence;
   - each KU has 2–4 Questions;
   - each KU has an independent ReviewState;
   - FTS has one row per KU;
   - AI jobs finish `completed` without half-written sibling KUs.
5. Verify Source Library shows all siblings under one Source.
6. Verify KU detail sibling navigation `此来源已内化 N 条知识`.

### Pass
Real full-answer internalization produces grounded independently reviewable KUs with Source/Evidence/Question/Review integrity.

## 6. Grounded judge: correct / partial / wrong

Use test-created KUs from section 5. Run three attempts:

1. **Correct**: answer closely from Source evidence in your own words.
2. **Partial**: include one important point but omit another.
3. **Wrong**: give a clearly incompatible but relevant answer.

For each verify:

- attempt row is saved before judgement;
- result is exactly `correct`, `partial`, or `wrong`;
- feedback is non-empty;
- cited/returned evidence belongs to the stored allowed Evidence set; no invented quote is accepted;
- mastery delta matches current policy (+15 / +5 / -10 unless repository code intentionally changed and tests document it);
- scheduler fields update: `stability`, `difficulty`, `lapse_count`, `scheduled_days`, `last_result`, `next_review_at`;
- exactly one `review_scheduler_events` row exists for the attempt;
- wrong becomes due immediately;
- no duplicate scheduler event exists for the same attempt.

### Pass
All three judgement classes produce evidence-grounded feedback and coherent scheduler updates.

## 7. AI Review Plan E2E

1. Open Review Center and use `智能安排`.
2. Verify the plan references only supplied due IDs, has no duplicates, selected count equals target count, every analyzed candidate is selected or deferred, and reasons are non-empty.
3. Confirm preview itself does **not** change `next_review_at`.
4. Apply the plan and verify session order matches selected order.
5. Verify deferred items remain due in the original queue.
6. If possible through a normal app action, change one due candidate between preview/apply and verify stale `queue_revision` rejection. Do not edit SQLite directly unless no production path exists.

### Pass
AI prioritizes the bounded session without silently rescheduling deferred knowledge.

## 8. Hybrid semantic search E2E

1. Ensure several test KUs have semantically related but lexically different wording.
2. Search a phrase directly matching one KU.
3. Search a conceptually equivalent phrase with weak literal overlap to encourage expansion.
4. Verify candidate count is bounded; expanded terms obey max/dedup rules; rerank returns only supplied IDs; score stays within 0..1; result limit is respected; reasons are non-empty and do not invent facts; ordinary Library search remains independent.
5. Test model-unavailable fallback only if it can be done without overwriting user credentials; otherwise mark this subcase `not executed`.

### Pass
Semantic search returns plausible ranked knowledge while validator boundaries hold.

## 9. AI Librarian E2E

Use test-created KUs where possible.

1. Open AI Librarian / organization flow.
2. Request organization/relationship suggestions.
3. Verify suggestions reference only existing IDs.
4. Preview then explicitly apply reversible actions such as Topic assignment/creation, Tag add, or a supported relation.
5. Verify AI changes have expected agent/curation provenance and appear in Audit Log where designed.
6. Verify archived/deleted/stale targets invalidate stale proposals.
7. Verify merge/split/delete/restore-backup style high-risk actions are **not** silently executed and still require the repository confirmation path.

### Pass
AI Librarian can execute approved reversible organization actions while destructive authority stays gated.

## 10. Cross-feature consistency

After sections 5–9 verify the same test records are consistent across:

- Today / Knowledge detail;
- ordinary Library search;
- Source Library;
- Topics / Tags;
- Review Center;
- Knowledge Health;
- Knowledge Graph;
- Semantic Search;
- Data & Backup / Audit Log.

Assert sibling KUs are independent in Library/Review but grouped in Source detail; Graph can center on one KU and expose relation/source/topic/tag reasons; Health reacts deterministically; Archive/Trash excludes units from active Graph/Review; Trash restore re-exposes them without deleting Source.

## 11. Network/API failure observations

Do **not** sabotage real credentials. For naturally occurring failures record only:

- feature/action;
- provider/protocol/model name;
- HTTP status or normalized error kind;
- whether output had started;
- whether fallback occurred;
- whether partial output was preserved;
- whether DB state remained transactionally consistent.

Redact Authorization, API key, Access Secret, cookies and tokens.

## 12. Final regression

From repository root run:

```powershell
pnpm check
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

Then run the repository's precise staged secret scan. Talisman low-severity hits such as JSX `key={...}` or SQL `PRIMARY KEY` are not evidence of a real secret; report precise-scan results separately.

Do not commit unrelated files.

## 13. Cleanup

1. Keep the manual pre-test backup.
2. Move only clearly test-created KUs to Trash if cleanup is desired.
3. Do not empty Trash.
4. Do not delete Source records directly.
5. Do not delete backups.
6. Leave real user knowledge untouched.

## 14. Required final report

### Environment
- HEAD:
- App version:
- AI route tested:
- Zhihu credential configured: yes/no
- DB schema:

### Results
| Test group | Result | Key evidence |
| --- | --- | --- |
| AI connectivity | PASS/FAIL/BLOCKED | ... |
| Zhihu search/Reader | PASS/FAIL/BLOCKED | ... |
| Full-answer internalization | PASS/FAIL/BLOCKED | ... |
| Grounded judgement | PASS/FAIL/BLOCKED | ... |
| AI review plan | PASS/FAIL/BLOCKED | ... |
| Semantic search | PASS/FAIL/BLOCKED | ... |
| AI Librarian | PASS/FAIL/BLOCKED | ... |
| Cross-feature consistency | PASS/FAIL/BLOCKED | ... |
| Final regression | PASS/FAIL | ... |

### Created test records
List IDs only. Never include credentials.

### Defects found
For each defect include severity, reproduction, expected, actual, likely component, and whether it was fixed.

### Remaining blockers
Only genuine blockers such as missing credential, external rate limit or third-party outage.

### Final verdict
Use one of:
- `READY FOR RELEASE`
- `READY WITH NON-BLOCKING ISSUES`
- `NOT READY`

Do not claim `READY` if a release-blocking E2E path failed or remained untested because required credentials were missing.
