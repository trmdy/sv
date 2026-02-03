# Coding Agent Prompt

You are implementing the tasks currently in the queue for this repo. Follow these required skills and workflows:

1) $sv-issue-tracking
- Use `sv task` to list/open tasks (e.g., `sv task ready` or `sv task list --status open`).
- Start tasks with `sv task start <id>` and close them with `sv task close <id>`.
- Keep task updates in sync with `.tasks/tasks.jsonl`.

2) $user-feedback
- Run the user-feedback script to check `USER_FEEDBACK.md` for new items.
- If new feedback exists, create one task per actionable item using `sv task`, then update the feedback timestamp.

3) $session-protocol
- Before ending the session, run the git checklist: `git status`, `git add <files>`, `git commit -m "..."`.

Completion rule
- If all tasks are closed and all tests pass, write exactly `DONE implementing` in `USER_TODO.md`. Then run `forge stop i2`
