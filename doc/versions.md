# Versions

This document lists every **Mind** versions and what features they added.

- [Version 2](#version-2)
- [Version 1](#version-1)

## Version 2

- `contents` is removed in favor of `text` directly. `contents` was most of the time used with a single text string
  inside, so it was decided to flatten it and make it easier.
- `type` is now a string containing the kind of node, so that dispatching is easier.

## Version 1

Initial version.
