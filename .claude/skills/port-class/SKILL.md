---
name: port-class
description: Plans and runs the porting of remaining build classes with class-porter agents - picks independent subtrees, briefs agents, merges their branches, verifies and updates the PR. Use when asked to port classes Cnnn or to continue the simulator's build coverage.
---

# Port classes

1. **Pick work.** Read `Docs/Plan.md` (status by class) and `.claude/Handoff.md`. A class can start only when its parent class is ported (parents in `Docs/BuildPlan.md`). Group classes into independent subtrees; bundle constant-only, low-risk classes into one agent; give each high-risk rewrite (35+ files) its own agent.
2. **Brief.** One worktree per agent: `git worktree add -b <name> Cache/Wt/<name> <integration HEAD>`. Spawn `class-porter` with the class ids, the worktree, the branch and which classes other agents hold. At most 3 agents at once (4 cores, about 15 GB RAM, limited disk).
3. **Merge.** When an agent reports, merge its branch into the integration branch, resolve conflicts (union `Change` variants, keep each branch's tests and docs rows), run the checks from `CLAUDE.md`, rebuild the release binary, and verify every ported class with `Tools/VerifyClass.sh` (or a regression sample when time is short).
4. **Publish.** Push only the user's designated branch, update the PR status table, and remove the merged worktree and branch.
5. **Report** to the user in their language: classes done, pass counts, open items.
