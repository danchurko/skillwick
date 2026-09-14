class Skillwick < Formula
  desc "Find relevant installed local skills"
  homepage "https://github.com/danchurko/skillwick"
  version "0.2.2"

  if Hardware::CPU.arm?
    url "https://github.com/danchurko/skillwick/releases/download/v0.2.2/skillwick-aarch64-apple-darwin.tar.xz"
    sha256 "3a2ff7ecbf8423802a6703a5be301d6cc345c72b8bb415b7952a00849e864ad8"
  else
    url "https://github.com/danchurko/skillwick/releases/download/v0.2.2/skillwick-x86_64-apple-darwin.tar.xz"
    sha256 "3bbf51f67ee9acac7e9b1e9d7edfaaca5c029516af1594854049a6c0d410ae6f"
  end

  def install
    bin.install "skillwick"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/skillwick --version")
  end
end
