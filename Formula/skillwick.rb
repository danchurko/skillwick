class Skillwick < Formula
  desc "Find relevant installed local skills"
  homepage "https://github.com/danchurko/skillwick"
  version "0.5.0"

  if Hardware::CPU.arm?
    url "https://github.com/danchurko/skillwick/releases/download/v0.5.0/skillwick-aarch64-apple-darwin.tar.xz"
    sha256 "73807cff8cfe4b92c8015a269b24fca134cc90e3d6477760a228ae770f3b2e6a"
  else
    url "https://github.com/danchurko/skillwick/releases/download/v0.5.0/skillwick-x86_64-apple-darwin.tar.xz"
    sha256 "de07ebdb9e64fd81362c78771baa9eeae06620534c89a860fab2d990889f6d45"
  end

  def install
    bin.install "skillwick"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/skillwick --version")
  end
end
