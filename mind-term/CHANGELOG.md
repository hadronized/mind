# v0.2

## v0.2.1-dev

- Fix for string-based inputs; they are now trimmed correctly.

## v0.2.0

> Thu Mar 23 2023

- Fix `mind paths` prefixing / auto create mode (we don’t auto create anymore here, it doesn’t make sense).
- Fix path selection returning `/` when aborting with empty string.
- Add `mind insert` shortcuts to create and open data at the same time.
- Add `mind ls` to list every trees.

# v0.1

## v0.1.1

> Tue Mar 21 2023

- Fix missing configuration file. The behavior is to start with `Config::default()`.
- If any error is encountered, exit with `1`.

## v0.1.0

- Initial revision.
