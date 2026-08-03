---
default: changed
---

# Releases are cut from change files, not from hand-edited version numbers

Every pull request now documents its own user-visible changes in a
`.changeset/` file, and CI refuses a pull request that adds none. Merging to
`main` opens a release pull request that compiles those files into
`CHANGELOG.md`, bumps the workspace version and empties `.changeset/`; merging
*that* tags the release and runs the whole publish pipeline in one go.
