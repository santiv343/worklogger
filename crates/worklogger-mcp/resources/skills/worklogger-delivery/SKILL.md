---
name: worklogger-delivery
description: Use when the user asks to finish a branch, create or update a pull request, and record delivery evidence with Worklogger MCP.
---

# Deliver work with Worklogger

Inspect the branch, working tree, diff, relevant validation and existing pull
requests before proposing a delivery. Use native Git for local commits and
pushes; use Worklogger for Jira and Bitbucket operations when those tools are
available.

For a new Bitbucket pull request, let Worklogger resolve effective default
reviewers. Show the visible preview exactly, including source and destination
branches, reviewers and `closeSourceBranch`. Do not delete the source branch
unless that choice is explicit. Reuse established language, evidence and Jira
conventions; ask only for genuinely missing decisions.
