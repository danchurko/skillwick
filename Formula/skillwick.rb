class Skillwick < Formula
  desc "Find relevant installed local skills"
  homepage "https://github.com/danchurko/skillwick"
  version "0.1.0"

  on_arm do
    url "https://github.com/danchurko/skillwick/releases/download/v0.1.0/skillwick-aarch64-apple-darwin.tar.xz"
    sha256 "86a24cd4d6653f752a44f00c5b4ff98081786404f372af539e3f1f3eec591506"
  end

  on_intel do
    url "https://github.com/danchurko/skillwick/releases/download/v0.1.0/skillwick-x86_64-apple-darwin.tar.xz"
    sha256 "dcc272ae3d486dc6e5d6ec0d18903095ab5f09879f6271e2fd2e25c747e9196b"
  end

  def install
    bin.install "skillwick"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/skillwick --version")
  end
end
