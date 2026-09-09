---
name: worklogger-jira
description: Use when the user needs to inspect or change Jira issues or own worklogs through the Worklogger MCP.
---

# Jira with Worklogger

Use the smallest available Worklogger Jira tool. Do not use deprecated toolkit
tools or invent board IDs, transition IDs, field IDs or an author identity.

For any write, first request `confirmed: false`. Paste
`confirmation.visiblePreview` under a visible `Vista previa` heading, then ask
for one explicit confirmation and repeat the unchanged request with its
single-use token. For worklogs, surface duplicate candidates before confirming.

When evidence is requested, add only reproducible command output in an
`EVIDENCIA:` block. Do not invent pending QA or risk sections.
