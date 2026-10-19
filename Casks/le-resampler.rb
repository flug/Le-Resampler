cask "le-resampler" do
  version "0.1.18"
  sha256 "1633e3788e35a12208d8a2c1527e750e4279ec61557810d8d4e2d089aa95e0ae"

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
