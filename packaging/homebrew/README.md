# Homebrew Packaging

This directory contains the formula template for a Homebrew tap.

## First Release

1. Commit and push the release-ready repository.
2. Create and push a version tag:

   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```

3. Compute the source tarball SHA256:

   ```sh
   curl -L https://github.com/ronenmagid/terminal-invaders/archive/refs/tags/v0.1.0.tar.gz | shasum -a 256
   ```

4. Copy `terminal-invaders.rb` into a tap repository such as `ronenmagid/homebrew-tap` under `Formula/terminal-invaders.rb`.
5. Replace `REPLACE_WITH_RELEASE_TARBALL_SHA256` with the computed SHA256.

## Local Formula Test

From the tap repository:

```sh
brew install --build-from-source ./Formula/terminal-invaders.rb
brew test ./Formula/terminal-invaders.rb
brew audit --strict --online ./Formula/terminal-invaders.rb
```

Users can install from the tap with:

```sh
brew install ronenmagid/tap/terminal-invaders
```
