# Collaborating on Lifeinvader

The shared upstream is `reycarrorg/lifeinvader-creator-hub`. This repository is private: the owner must invite a collaborator and they must accept the invitation before they can see or clone it. A link alone does not grant access. Do not paste private contents or credentials into public issues, commits, or chats.

## Choose where to work

The public repository can be forked, or collaborators with write access can create a separate branch directly in the shared repository. Both approaches use pull requests back into the upstream `main` branch. A fork is a separate repository; a branch is a line of work within a repository. You do not need to repeatedly fork each other's forks.

### Fork workflow

1. On the upstream repository, choose **Fork** and create your own fork. If GitHub does not offer this option, use the shared-branch workflow below.
2. Clone your fork, then add the original as `upstream`:

   ```bash
   git clone https://github.com/YOUR-USERNAME/lifeinvader-creator-hub.git
   cd lifeinvader-creator-hub
   git remote add upstream https://github.com/reycarrorg/lifeinvader-creator-hub.git
   git fetch upstream
   ```

3. Before starting new work, update a clean local `main` using a fast-forward merge from upstream, then make a focused branch:

   ```bash
   git switch main
   git fetch upstream
   git merge --ff-only upstream/main
   git switch -c codex/short-description
   ```

4. Make and test the change. Update `docs/PROGRESS.md` and, if relevant, `docs/ISSUES.md` in the same change. Commit, push the branch to your fork, and open a pull request against `reycarrorg/lifeinvader-creator-hub:main`.

   ```bash
   git add <changed-files>
   git commit -m "Describe the change"
   git push -u origin codex/short-description
   ```

5. Let the other collaborator review the pull request. Resolve feedback and merge only when the repository owner or collaborator with merge access decides it is ready. After merge, sync your fork before your next task. If `--ff-only` fails, inspect the branch history and resolve it deliberately; do not force-push over anyone else's work.

### Shared-branch workflow

Clone the upstream repository with the GitHub account that accepted the invitation. Fetch and fast-forward `main`, create a focused `codex/short-description` branch, commit, and push that branch to `origin`. Open a pull request into `main`. The repository owner can use this same workflow. Coordinate overlapping work before changing the same files.

## Issues and progress

- Start from `docs/TODO.md` for the shared agenda. Either collaborator can propose a different direction in a branch or fork; agree on shared defaults before merging. Keep the checklist current as work changes.
- Keep `docs/PROGRESS.md` as a concise, dated handoff: what changed, what was actually tested, and what remains. Add meaningful entries rather than a play-by-play.
- Keep `docs/ISSUES.md` as the in-repository list of known problems and planned work. Record reproduction steps and evidence when known. Distinguish confirmed behavior from assumptions.
- Use the upstream repository's GitHub **Issues** for individual tasks and bugs when available. Include an issue or pull request link in the tracked documents once it exists. GitHub issues are associated with the repository on GitHub; their text is not automatically a file in Git.
- Link pull requests to relevant issues, describe the resulting behavior, and include the tests run and any limitations. Do not mark an issue fixed until the change is verified.
- AI assistants should read `AGENTS.md`, `TODO.md`, and the handoff documents at the start of a task and leave them current when finishing substantive work. Follow the current user's requested direction while making shared changes reviewable.

## Local data and testing

Each clone uses its own database, profiles, and media. Git shares code and documentation, not local app data. Keep `data/`, `media/`, downloader binaries, archives, credentials, and account exports out of commits. Use synthetic test files and fake downloaders. See the [README](../README.md) for setup and tests.

References: [Fork a repository](https://docs.github.com/en/pull-requests/how-tos/work-with-forks/fork-a-repo), [Sync a fork](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/syncing-a-fork), and [Create an issue](https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/creating-an-issue).
