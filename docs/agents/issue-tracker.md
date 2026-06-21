# Issue Tracker: GitHub

Issues and PRDs for this repo live in GitHub Issues for `realmorrisliu/Alan`.
Use the `gh` CLI with `--repo realmorrisliu/Alan` for all issue operations.

## Conventions

- Create an issue: `gh issue create --repo realmorrisliu/Alan --title "..." --body "..."`
- Read an issue: `gh issue view <number> --repo realmorrisliu/Alan --comments`
- List issues: `gh issue list --repo realmorrisliu/Alan --state open --json number,title,body,labels,comments`
- Comment on an issue: `gh issue comment <number> --repo realmorrisliu/Alan --body "..."`
- Apply a label: `gh issue edit <number> --repo realmorrisliu/Alan --add-label "..."`
- Remove a label: `gh issue edit <number> --repo realmorrisliu/Alan --remove-label "..."`
- Close an issue: `gh issue close <number> --repo realmorrisliu/Alan --comment "..."`

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Run `gh issue view <number> --repo realmorrisliu/Alan --comments`.
