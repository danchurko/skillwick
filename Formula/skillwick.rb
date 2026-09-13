class Skillwick < Formula
  desc "Find relevant installed local skills"
  homepage "https://github.com/churdaa/skillwick"
  version "0.1.4"

  if Hardware::CPU.arm?
    url "https://github.com/churdaa/skillwick/releases/download/v0.1.4/skillwick-aarch64-apple-darwin.tar.xz"
    sha256 "fc51722e26229bc6a78efc670495c6dcf9367d74a9700f4ace6f80fe7c8f24fe"
  else
    url "https://github.com/churdaa/skillwick/releases/download/v0.1.4/skillwick-x86_64-apple-darwin.tar.xz"
    sha256 "0723cd48b6324a21957a379094255bc7e5357d69411ab66c13bb876375721ba2"
  end

  def install
    bin.install "skillwick"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/skillwick --version")
  end
end
