# Terminal Invaders

A Rust terminal Space Invaders-style arcade game.

## Run

```sh
cargo run
```

## Install

Build from source with Cargo:

```sh
cargo install --git https://github.com/ronenmagid/terminal-invaders
```

After a Homebrew tap is published:

```sh
brew install ronenmagid/tap/terminal-invaders
```

## Release

Releases are tagged with semantic versions:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The release workflow builds macOS, Linux, and Windows binaries. The Homebrew
formula template lives in `packaging/homebrew/`.

## Controls

- `Enter`: start / restart
- `Left` / `Right`: move
- `Space`: fire
- `P`: pause
- `S`: sound on/off
- `Q` or `Esc`: quit

The game uses ANSI terminal rendering and raw keyboard input via `crossterm`.
Sound effects use the terminal bell, so they depend on your terminal's audible bell setting.
