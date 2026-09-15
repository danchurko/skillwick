class Skillwick < Formula
  desc "Find relevant installed local skills"
  homepage "https://github.com/danchurko/skillwick"
  version "0.3.0"

  if Hardware::CPU.arm?
    url "https://github.com/danchurko/skillwick/releases/download/v0.3.0/skillwick-aarch64-apple-darwin.tar.xz"
    sha256 "346906a8ace286b21d3df666195580db86c40d98d7fa00ffb6964e581e07bb46"
  else
    url "https://github.com/danchurko/skillwick/releases/download/v0.3.0/skillwick-x86_64-apple-darwin.tar.xz"
    sha256 "4389b6644e28034b2bc3759b44639992b912acd3dbc57f0ffe46d19214b29606"
  end

  def install
    bin.install "skillwick"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/skillwick --version")
  end
end
