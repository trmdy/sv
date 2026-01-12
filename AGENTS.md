# AGENTS.md: Simultaneous Versioning `sv`

This repository is for the project `sv`: Simultaneous Versioning.

## Code structure

- Code lives in src/
- Tests live in tests/
- Business logic should have reasonable test coverage
- Files should be mostly small and atomic; split when large
- Simple easy-to-understand code

## UX

- Good user experience is the most important thing
- Simply commands
- Clear user stories
- Easy to use for robots (AI)

## Multi-agent workflow

- There are multiple agents working here.
- By default everyone works on `main`
- Use `sv take` to take ownership of files when your edits are comprehensive; avoid using exclusive locks unless you are refactoring entire files
    - Use short ttls (a few minutes)
- ignore changes you dont know. this is very common. just do your work and commit your changes.
- create only new workspaces with `sv` when there are great chances of conflicts with other agents.

## Docs

All documentation for the project can be found inside `docs/`.

## Tools

- Use `sv` for worktree management; `sv --robot-help`
    - Use `sv lease` to register leases on files your work on, or see other leases
    - Use `sv hoist` to merge your work from a workspace when finished. use `--close-tasks` if tasks are associated with ws
    -
- Use `sv tasks` for task management; `sv task --robot-help`
- Use `fmail` for interagent communication;
    - Use `fmail register` whenever starting new work to get your alias
    - Send messages using your current task id as topic. otherwise send on `global`.
    - Use `fmail watch` to wait for messages on a topic if you need confirmation from another agent.
