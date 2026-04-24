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
cargo run -p tg
```

## Quick Start

- `hjkl`, arrow keys, or `wasd` move the cursor
- `Space` enters drawing mode
- `e` enters erasing mode
- `Enter` or `v` edits the thing under the cursor
- `i` or `t` enters text mode
- `/` or `f` starts a connector
- `Y`, `X`, `I` export, cut, or import the whole document
- `?` opens help
- `q` quits

## License

Apache-2.0. See [LICENSE](LICENSE).
