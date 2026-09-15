# ZhiForge Hackathon Demo Runbook

This runbook is for the current demo path. The demo must use real Zhihu content, real persisted product data, and the normal ZhiForge flow; do not replace any step with seeded or fake knowledge records.

## Before the demo

- Start the ZhiForge desktop app normally.
- Confirm an AI Provider/model is configured and can answer a normal translation request.
- Configure the Zhihu Open Platform Access Secret in **设置 → 知乎搜索** if the demo will use in-app search.
- If the Browser Helper path will also be shown, load `browser-extension/` as an unpacked extension in the Chromium browser used for the demo.
- Keep one suitable Zhihu answer ready. Choose a passage with a clear claim and enough source text for 2–4 understanding questions.
- Open the ZhiForge home window once so **知乎搜索 / 继续学习 / 最近内化** are visible.

## Recommended continuous demo path: search → read → internalize

1. Start on the ZhiForge home page and search a real question in the top Zhihu search box.
2. Show the returned real Zhihu results. Briefly demonstrate **综合 / 赞同 / 讨论 / 最新** sorting if useful.
3. Open one result in the built-in Reader and select a valuable passage in the answer text.
4. Click **内化选中内容**. Confirm the Reader shows **已内化 · 稍后检查你的理解** (or **已经内化过** for a duplicate).
5. Return to the home page. The new KnowledgeUnit appears in **最近内化** immediately while AI processing runs in the background.
6. Open the KnowledgeUnit. Verify the source contains the Zhihu question/title, author, selected passage, context and original URL.
7. After Extract + Question Generation completes, answer one generated question deliberately incompletely or incorrectly.
8. Show the AI judgement: correct/missing/wrong points, source-grounded Evidence, and the mastery change. A wrong answer uses the real MVP rule `-10` and can move the unit into **薄弱**.
9. Click **回原文看这里** in the judgement. The source card must open automatically and highlight the exact Evidence span inside the selected passage.
10. For a Zhihu source, click **打开知乎原文** to return to the author’s original page when useful.
11. Return to ZhiForge and answer the same question again with the missing point corrected. `Ctrl+Enter` can submit quickly during the demo.
12. Show the new judgement and mastery increase. A correct answer uses the real MVP rule `+15`; partial uses `+5`.
13. If the KnowledgeUnit has another not-yet-correct question, use **下一题**. Otherwise continue to the next due KnowledgeUnit with **下一条知识**.
14. Return to the home page and show **继续学习**. Only KnowledgeUnits whose scheduled review time is due appear there; a newly internalized unit is not falsely treated as an immediate due review.

## Optional alternate entry: external Zhihu page + Browser Helper

1. Read a real Zhihu answer in the browser and select a valuable passage.
2. Wait for the ZhiForge selection toolbar and click **内化**.
3. Confirm the toolbar shows **已内化 · 知乎**. This proves the Browser Helper context was merged instead of falling back to a generic desktop Source.
4. Continue from the same KnowledgeUnit flow above: AI extraction → understanding question → wrong/partial answer → source Evidence → correction → persisted mastery/review state.

The selection toolbar also keeps **搜知乎** as the first action. Selecting text anywhere and clicking it opens the ZhiForge Zhihu search view with that text as the search seed.

## What the audience should understand

The key story is not “AI generated a quiz.” It is:

`找到真人经验 → 选中真正有价值的一段 → 内化 → AI 检查理解 → 暴露漏洞 → 回到真人原文核对 → 再次表达 → 掌握度变化 → 在正确时间进入复习`

AI is responsible for extraction, questioning and judgement. The Zhihu source remains the authority for the knowledge and the Evidence shown to the user.

## Demo acceptance checklist

The demo is valid only when all of these use persisted product data:

- In-app Zhihu search uses the real Open Platform response, or external Zhihu selection uses the normal desktop toolbar.
- **内化** saves the real selected text and source metadata.
- AI Extract produces a real KnowledgeUnit and exact Evidence.
- Question Generation produces real persisted questions.
- The user answer is saved before judgement.
- AI judgement is grounded only in the saved source Evidence.
- Wrong/partial/correct updates the persisted mastery and review state.
- Feedback Evidence can locate the exact original span.
- The original Zhihu URL can be reopened.
- A duplicate desktop Source can be upgraded with richer Zhihu metadata instead of creating a second KnowledgeUnit.
- Restarted app data remains available from SQLite.

## Fast troubleshooting

If the external-page toolbar says only **已内化** instead of **已内化 · 知乎**, the generic desktop fallback worked but Browser Helper enrichment did not. Check that the unpacked extension is enabled and ZhiForge is running so `127.0.0.1:17832` is available.

If in-app search reports that the Access Secret is missing, open **设置 → 知乎搜索** and save the Zhihu Open Platform Access Secret. The credential is stored in Windows Credential Manager, not in `config.json`.

If AI processing fails, the Source should still remain saved. Use **重新处理** after fixing the Provider; do not recreate or reset the local database.

If judgement fails after the answer was submitted, the Attempt should still be visible as a saved answer. Use **重新判题** rather than re-entering the answer.
