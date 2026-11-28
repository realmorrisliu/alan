# Issue Tracker: GitHub

Issues and PRDs for this repo live in GitHub Issues for `thinksmart-systems/alan`.
Use the `gh` CLI with `--repo thinksmart-systems/alan` for all issue operations.

## Conventions

- Create an issue: `gh issue create --repo thinksmart-systems/alan --title "..." --body "..."`
- Read an issue: `gh issue view <number> --repo thinksmart-systems/alan --comments`
- List issues: `gh issue list --repo thinksmart-systems/alan --state open --json number,title,body,labels,comments`
- Comment on an issue: `gh issue comment <number> --repo thinksmart-systems/alan --body "..."`
- Apply a label: `gh issue edit <number> --repo thinksmart-systems/alan --add-label "..."`
- Remove a label: `gh issue edit <number> --repo thinksmart-systems/alan --remove-label "..."`
- Close an issue: `gh issue close <number> --repo thinksmart-systems/alan --comment "..."`

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Run `gh issue view <number> --repo thinksmart-systems/alan --comments`.
