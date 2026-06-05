# Git integration

Operon ships a full Git panel in the sidebar — stage, commit, push, and
publish to GitHub without leaving the app.

![Git panel](../img/git-panel.png){ width=400 }

## Initialize

If the current folder has no `.git`, the Git panel shows an
**Initialize Repository** button. Click it to run `git init`.

## Stage, commit, push

The panel lists every modified, added, untracked, or deleted file. Click
the **+** next to a file to stage it, or **Stage all** to stage everything.
Write a commit message in the box at the top of the panel and click
**Commit**.

Push to the configured remote with **Push**. Pull / fetch are right there
too.

## Publish to GitHub

If you don't have a remote yet, click **Publish to GitHub**. Operon runs
`gh auth login` if needed (which opens your browser), then creates a new
GitHub repo and pushes the current branch.

Choose public or private at creation time.

## Diff viewing

Click any file in the changes list to see its diff side-by-side. Same
diff engine as the AI-edit reviewer in the editor.

## Branching & tags

The bottom of the Git panel shows the current branch — click it for a
branch picker (checkout existing, create new). Tags appear in their own
section; create new tags for milestone releases.

## What we use under the hood

Operon shells out to your installed **`git`** and **`gh`** CLIs — same as
running them in the integrated terminal. That means:

- Your `~/.gitconfig` settings (name, email, signing key) apply
- Your SSH agent / credential helper is honored
- Operon doesn't carry its own git implementation that might drift from
  the system one

You can keep using `git` from the terminal at any time; Operon picks up
the changes immediately.

## Working with branches

For PRs, the standard flow:

```bash
git checkout -b feature/my-change cross-platform
# edit, commit
git push -u origin feature/my-change
gh pr create --base cross-platform
```

The Git panel makes branch creation a click; `gh pr create` is still the
fastest way to open the PR from the terminal.
