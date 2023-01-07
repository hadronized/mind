# Architecture

**Mind** is organized in different crates, each of them allowing for a different layer of abstraction:

- `mind`: the central crate, which provides parsing **Mind** trees and expose basic operations on them. You can think
  of this crate as a way to access _every features_ without interactivity or a way to view results — or said 
  differently, accessing the features programmatically.
- `mind-term`: a crate adding interaction via a terminal.

## `mind`

The `mind` crate exports symbols to work with **Mind** trees. It contains the fundamental data structures as well as the
associated computations to execute on them.

## `mind-term`

`mind-term` adds interaction via a terminal. It does so by implementing a TUI and a CLI (in case the user wants to use
the data by composing it with other tools).

Additionally, it exposes configuration to customize how `mind-term` interacts with `mind`.