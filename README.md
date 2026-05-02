# tg

`tg` is a text diagram editor for the terminal.

WYSIWYG-style editing for plain-text diagrams.
No DSL, no hidden metadata, no custom format: just text in, text out.

Built for keyboard-driven structural editing, it lets you:

- draw boxes, custom shapes, and lines
- move, copy, cut, and paste
- resize shapes intelligently
- connect shapes with automatically routed arrows
- add and edit text labels
- keep diagrams in Markdown, code comments, and version control
- stay in the terminal instead of switching to a mouse-driven editor

```
┏tg━━━━━━━━━┓
┃           ┃
┃           ┃          ┏━━━━━━━━━━━━┓
┃           ┃─────────▷┃╺┳╸         ┃
┗━━━━━━━━━━━┛          ┃ ┃extagra△△ ┃
╺━━━━━━━━━━━╸          ┃      ╶──╯│ ┃
                       ┃          ╵ ┃
                       ┗━━━━━━━━━━━━┛
```

## Run

```bash
npm install -g @textagram/tg
tg
```

Or run it one-shot with:

```bash
npx @textagram/tg path/to/diagram.txt
```

For development from this monorepo:

```bash
cargo run -p tg
```

Open a file-backed editing session with:

```bash
cargo run -p tg -- path/to/diagram.txt
```

If the file contains a recognized `textagram` fence, `tg` edits the first fence
body and writes it back into the same markdown file on save. Otherwise it edits
the whole file as a plain diagram document.

## Source Builds

`tg` is open source, but source builds are not currently supported from the
public `tg` repository alone.

The terminal host depends on the Textagram core engine, which remains private
while the core is being refactored. Official binaries are built from the private
integration workspace and published through GitHub Releases and npm.

We intend to make standalone source builds possible once the core refactor is complete and
stable.

## Quick Start

- `hjkl`, arrow keys, or `wasd` move the cursor
- `Space` enters drawing mode
- `e` enters erasing mode
- `Enter` or `v` edits the thing under the cursor
- `i` or `t` enters text mode
- `/` or `f` starts a connector
- `Y`, `P`, `X` export, import, or cut the whole document
- `?` opens help
- `q` quits

## File Mode

- `tg FILE` opens a file-backed session
- `Ctrl+S` saves the file and keeps editing
- plain `q` exits immediately when clean
- plain `q` prompts `save changes? y/n/esc` when dirty
- `Q` writes a repro recording and exits without using the save prompt

## License

Apache-2.0. See [LICENSE](LICENSE).
