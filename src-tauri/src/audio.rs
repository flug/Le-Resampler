use rodio::{Decoder, DeviceSinkBuilder, Player};
use std::fs::File;
use std::io::BufReader;
use std::sync::mpsc;
use std::thread;

#[derive(Debug)]
enum AudioCmd {
    Play(String),
    Stop,
    Shutdown,
}

pub struct AudioPlayer {
    tx: mpsc::SyncSender<AudioCmd>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self, String> {
        let (tx, rx) = mpsc::sync_channel::<AudioCmd>(4);

        thread::Builder::new()
            .name("sampli-audio".into())
            .spawn(move || audio_thread(rx))
            .map_err(|e| e.to_string())?;

        Ok(Self { tx })
    }

    pub fn play(&self, path: String) -> Result<(), String> {
        self.tx
            .try_send(AudioCmd::Play(path))
            .map_err(|e| format!("Audio channel error: {}", e))
    }

    pub fn stop(&self) -> Result<(), String> {
        self.tx
            .try_send(AudioCmd::Stop)
            .map_err(|e| format!("Audio channel error: {}", e))
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        let _ = self.tx.try_send(AudioCmd::Shutdown);
    }
}

#[cfg_attr(tarpaulin, coverage(off))]
fn audio_thread(rx: mpsc::Receiver<AudioCmd>) {
    // MixerDeviceSink owns the audio output stream — keep it alive for the lifetime of this thread.
    // Player is connected to its mixer and controls playback.
    let sink_handle = match DeviceSinkBuilder::open_default_sink() {
        Ok(h) => h,
        Err(e) => {
            log::error!("Cannot open audio device: {}", e);
            // Drain the channel before exiting so senders don't block
            while rx.recv().is_ok() {}
            return;
        }
    };

    let player = Player::connect_new(sink_handle.mixer());

    while let Ok(cmd) = rx.recv() {
        match cmd {
            AudioCmd::Play(path) => {
                player.clear();

                let file = match File::open(&path) {
                    Ok(f) => f,
                    Err(e) => {
                        log::error!("Cannot open audio file {:?}: {}", path, e);
                        continue;
                    }
                };

                match Decoder::try_from(BufReader::new(file)) {
                    Ok(decoder) => {
                        player.append(decoder);
                        log::debug!("Playing: {}", path);
                    }
                    Err(e) => {
                        log::error!("Cannot decode {:?}: {}", path, e);
                    }
                }
            }
            AudioCmd::Stop => {
                player.clear();
                log::debug!("Playback stopped");
            }
            AudioCmd::Shutdown => {
                player.stop();
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_player_new_succeeds() {
        AudioPlayer::new().expect("should not panic on spawn");
    }

    #[test]
    fn audio_player_play_executes_send_path() {
        let p = AudioPlayer::new().unwrap();
        let _ = p.play("irrelevant.wav".to_string());
    }

    #[test]
    fn audio_player_stop_executes_send_path() {
        let p = AudioPlayer::new().unwrap();
        let _ = p.stop();
    }

    #[test]
    fn audio_player_drop_no_panic() {
        let p = AudioPlayer::new().unwrap();
        drop(p);
    }
}
