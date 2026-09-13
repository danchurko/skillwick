class Skillwick < Formula
  desc "Find relevant installed local skills"
  homepage "https://github.com/churdaa/skillwick"
  version "0.1.5"

  if Hardware::CPU.arm?
    url "https://github.com/churdaa/skillwick/releases/download/v0.1.5/skillwick-aarch64-apple-darwin.tar.xz"
    sha256 "ecac5d57e653204c296d558faf4e40edbf45aae4256a7fe00b512b4bc3bdff65"
  else
    url "https://github.com/churdaa/skillwick/releases/download/v0.1.5/skillwick-x86_64-apple-darwin.tar.xz"
    sha256 "e1d2fe3781ecd779adebee6cb6233a28a0948995b442f4459cfd93bccfcbf098"
  end

  def install
    bin.install "skillwick"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/skillwick --version")
  end
end
