# Terminal Invaders

A Rust terminal Space Invaders-style arcade game.

## Run

```sh
cargo run
```

## Controls

- `Enter`: start / restart
- `Left` / `Right`: move
- `Space`: fire
- `P`: pause
- `S`: sound on/off
- `Q` or `Esc`: quit

The game uses ANSI terminal rendering and raw keyboard input via `crossterm`.
Sound effects use the terminal bell, so they depend on your terminal's audible bell setting.
