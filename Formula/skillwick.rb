class Skillwick < Formula
  desc "Find relevant installed local skills"
  homepage "https://github.com/danchurko/skillwick"
  version "0.4.0"

  if Hardware::CPU.arm?
    url "https://github.com/danchurko/skillwick/releases/download/v0.4.0/skillwick-aarch64-apple-darwin.tar.xz"
    sha256 "ad517c94dab39b146f351e78e753394d9a870b1187858ba554289cc711f90d27"
  else
    url "https://github.com/danchurko/skillwick/releases/download/v0.4.0/skillwick-x86_64-apple-darwin.tar.xz"
    sha256 "9d2fc0a9252c47a31d50072aae11a3ae9eee01a90f97baa662de4635f6e180a6"
  end

  def install
    bin.install "skillwick"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/skillwick --version")
  end
end
