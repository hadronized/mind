# Mind, the power of trees at your fingertips

**Mind** is an organizer tool. It can be used to accomplish and implement a wide variety of workflows. It is
designed to quickly add items in trees. _Why a tree?_ Well, list of things like TODO lists are great but they lack the
organization part. Most of them can be gathered in “lists of lists” — you probably have that on your phone. A list of
list is basically a tree. But editing and operating a list of list is annoying, so it’s better to have a tool that has
the concept of a node and a tree as a primitive.

**Mind** trees can be used to implement workflows like:

- Journaling. Have a node for each day, which parent will be the month, which parent will be the year, etc.
- Note taking. You are in the middle of a meeting and you heard something important? Don’t write that in a Markdown
  document in your `~/documents` that is probably alreaddy a mess: open your **Mind** tree and add it there!
- “Personal wiki.” Because of the nature of a tree, it is convenient to organize your personal notes about your work
  services, other teams’ products, OKRs, blablabla by simply creating trees in trees!
- Task management. Why not having a tasks tree with three or four sub-trees for your backlog, on-going work, finished
  work and cancelled tasks? It’s all possible!

The possibilities are endless.

# Features

**Mind** features two main concepts; global trees and local trees:

- A global tree is a tree that is unique to your machine / computer. Opening your main **Mind** tree from Neovim will
  always open and edit that tree. It’s basically your central place for your **Mind** nodes.
- A local tree is a tree that is relative to a given directory. **Mind** implements a `cwd`-based local form of tree, so
  you can even share those trees with other people (as long as they use **Mind** as well).

Atop of that, **Mind** has the concept of “project” trees, which are either a global tree, or a local tree. A global
project tree is stored at the same place as your main tree and the purpose of such a tree is to be opened only when your
`cwd` is the same as the tree, but you don’t want the tree to be in the actual `cwd`. That can be the case if you work
on a project where you don’t want to check the tree in Git or any versioning system.

On the other side, a local project tree is what it means: it lives in the `cwd`, under `.mind`, basically.

Besides that, **Mind** allows you to manipulate trees and nodes. Feature set:

- Everything is interactive.
- Cursor-base interaction. Open a tree and start interacting with it!
  - Expand / collapse nodes.
  - Add a node to a tree by adding it before or after the current node, or by adding it inside the current node at the
    beginning or end of its chilren.
  - Rename the node under the cursor.
  - Change the icon of the node under the cursor.
  - Delete the node under the cursor with a confirmation input.
  - Select a node to perform further operations on it.
  - Move nodes around!
  - Select nodes by path! — e.g. `/Tasks/On-going/3345: do this`
- Supports user keybindings via keymaps. Keymaps are namespaced keybindings. They keymaps are fixed and defined by
  **Mind**, and users can decide what to put in them. For instance, you have the _default_ keymap for default
  navigation, _selection_ keymap for when a node is selected, etc. etc.
- Nodes are just text, icons and some metadata by default. You can however decide to associate them with a _data file_,
  for which the type is user-defined (by default Markdown), or you can turn them into URL nodes.
- A data node will open its file when triggered.
- A URL node will open its link when triggered.
- More to come!
