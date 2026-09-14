class Skillwick < Formula
  desc "Find relevant installed local skills"
  homepage "https://github.com/danchurko/skillwick"
  version "0.2.0"

  if Hardware::CPU.arm?
    url "https://github.com/danchurko/skillwick/releases/download/v0.2.0/skillwick-aarch64-apple-darwin.tar.xz"
    sha256 "a4a9cfff5b91c215f784d1f5fba4896350a133343f5544f974d6d589dd90172d"
  else
    url "https://github.com/danchurko/skillwick/releases/download/v0.2.0/skillwick-x86_64-apple-darwin.tar.xz"
    sha256 "a6d9ea3f98f1635495b32f931c6938290005cb88e47816c176557f4e0d778a77"
  end

  def install
    bin.install "skillwick"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/skillwick --version")
  end
end
