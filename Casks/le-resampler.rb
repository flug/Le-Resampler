cask "le-resampler" do
  version "0.1.16"
  sha256 "f5d8f2fbd30928aec5c4bb76c8e361a999a720b878fe04c81a97a1e2f555eb0c"

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
