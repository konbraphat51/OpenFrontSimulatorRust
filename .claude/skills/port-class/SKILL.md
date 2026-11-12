---
name: port-class
description: Plans and runs the porting of remaining build classes with class-porter agents - picks independent subtrees, briefs agents, merges their branches, verifies and updates the PR. Use when asked to port classes Cnnn or to continue the simulator's build coverage.
---

# Port classes

1. **Pick work.** Read `Docs/Plan.md` (status by class) and `.claude/Handoff.md`. Parents are in `Docs/BuildPlan.md`. Do not wait for a parent class to merge: start a child class from the parent agent's branch (pipelining) and re-merge when the parent changes. Group classes into independent subtrees; bundle constant-only, low-risk classes into one agent; give each high-risk rewrite (35+ files) its own agent.
2. **Brief.** One worktree per agent: `git worktree add -b <name> Cache/Wt/<name> <integration HEAD>`. Spawn `class-porter` with the class ids, the worktree, the branch and which classes other agents hold. At most 3 local agents at once (4 cores, about 15 GB RAM, limited disk). For more parallelism, start remote cloud sessions (`create_session` with this repository, the integration branch as `source_revision` and an `outcome_branch` of `claude/port-<class>`); each sets itself up from `.claude/Handoff.md` (Environment setup), fetches its own records and pushes only its outcome branch, which you merge and verify locally.
3. **Merge.** When an agent reports, merge its branch into the integration branch, resolve conflicts (union `Change` variants, keep each branch's tests and docs rows), run the checks from `CLAUDE.md`, rebuild with `--profile verify`, run `Tools/QuickRegression.sh` after each merge and the full `Tools/VerifyClass.sh` set before publishing.
4. **Publish.** Push only the user's designated branch, update the PR status table, and remove the merged worktree and branch.
5. **Report** to the user in their language: classes done, pass counts, open items.
