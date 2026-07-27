cask "le-resampler" do
  version "0.1.6"
  sha256 "b2fedf35ce7b706f76ff931fb13c507eb75c9cea25741f4c913d36cb7397c12f"

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
