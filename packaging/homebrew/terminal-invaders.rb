class TerminalInvaders < Formula
  desc "Terminal Space Invaders-style arcade game"
  homepage "https://github.com/ronenmagid/terminal-invaders"
  url "https://github.com/ronenmagid/terminal-invaders/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_RELEASE_TARBALL_SHA256"
  license "MIT"
  head "https://github.com/ronenmagid/terminal-invaders.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/terminal-invaders --version")
  end
end
