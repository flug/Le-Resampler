cask "le-resampler" do
  version "0.1.19"
  sha256 "f3257328023545ace94476f166c2dcbfc48444f57fc2655a4bbb4a52f69e7afb"

  url "https://github.com/flug/Le-Resampler/releases/download/v#{version}/Le%20Resampler_#{version}_universal.dmg"
  name "Le Resampler"
  desc "Audio sample manager for music producers and Akai MPC Sample users"
  homepage "https://github.com/flug/Le-Resampler"

  app "Le Resampler.app"

  zap trash: [
    "~/Library/Application Support/com.sampli.leresampler",
    "~/Library/Preferences/com.sampli.leresampler.plist",
  ]
end
