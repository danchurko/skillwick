class Skillwick < Formula
  desc "Find relevant installed local skills"
  homepage "https://github.com/danchurko/skillwick"
  version "0.2.1"

  if Hardware::CPU.arm?
    url "https://github.com/danchurko/skillwick/releases/download/v0.2.1/skillwick-aarch64-apple-darwin.tar.xz"
    sha256 "f5ba5c90845c438047e6b2d7f80117ccfac0bb3e41725abdad8dc7f13cbbfa5b"
  else
    url "https://github.com/danchurko/skillwick/releases/download/v0.2.1/skillwick-x86_64-apple-darwin.tar.xz"
    sha256 "b408bc0d1f63ead914639e554a7b7e4a6f7a5b9d6ba8180847939a4610dfd620"
  end

  def install
    bin.install "skillwick"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/skillwick --version")
  end
end
