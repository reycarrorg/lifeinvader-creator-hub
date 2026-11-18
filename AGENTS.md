# Instructions for collaborators and AI assistants

Lifeinvader Creator Hub is a Rust/Axum local content hub. Before substantial work, read `README.md`, `docs/TODO.md`, `docs/COLLABORATION.md`, `docs/PROGRESS.md`, and `docs/ISSUES.md` and inspect the current code and Git state; repository notes can become stale.

- Keep `reycarrorg/lifeinvader-creator-hub` as the shared upstream. Work on a focused branch in your fork or the shared repository and propose changes with a pull request. Avoid overwriting another collaborator's work.
- At the end of substantive work, update `docs/TODO.md` and `docs/PROGRESS.md` with the change, validation performed, and any remaining work. Update `docs/ISSUES.md` when an issue is found, resolved, or changed; link related GitHub issues and pull requests when they exist. Do not invent IDs or mark unverified work complete.
- Protect local data: do not commit `data/`, `media/`, `tools/`, local download archives, credentials, cookies, private profile data, or personal source videos. Keep a user's existing database and media when developing or testing.
- The app binds to localhost and has no shared-user authentication. Do not expose it to a network as part of routine development.
- Do not re-download archived media merely to build thumbnails. Keep each profile's download archive and original post IDs intact, and avoid copying media unnecessarily when building the future chooser.
- Test with disposable data and fake downloaders. Do not start a live sync or change social account settings without the user's instruction. Report actual tests and external-platform limitations clearly.
- The project owner and collaborators choose what to build, merge, publish, or share. Follow the current user's instructions and preserve existing work.

- This is a shared project, and collaborators may adapt their own forks. Treat the agenda as current context and a proposed sequence, not as a substitute for the current user's instructions. Distinguish shared upstream decisions from a collaborator's own preferences.
