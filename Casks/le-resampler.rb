cask "le-resampler" do
  version "0.1.21"
  sha256 "a93bad587e945e6d8b7a7531f3d3120cc680f2546ef12320d16556870e41fcc0"

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
