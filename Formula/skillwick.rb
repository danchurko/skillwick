class Skillwick < Formula
  desc "Find relevant installed local skills"
  homepage "https://github.com/danchurko/skillwick"
  version "0.1.2"

  if Hardware::CPU.arm?
    url "https://github.com/danchurko/skillwick/releases/download/v0.1.2/skillwick-aarch64-apple-darwin.tar.xz"
    sha256 "7d23d88874053769f66d8b045ff1e6eaf7c9c751ab0500306ff53190af6e06f4"
  else
    url "https://github.com/danchurko/skillwick/releases/download/v0.1.2/skillwick-x86_64-apple-darwin.tar.xz"
    sha256 "807e3dc1bb5e440f8538b8ff5f548ef06207bc66aebcf0c2b111b9dfb8745c31"
  end

  def install
    bin.install "skillwick"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/skillwick --version")
  end
end
