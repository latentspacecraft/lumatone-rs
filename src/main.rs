mod midi;

use std::time::Duration;

use midi::commands::SetKeyColor;
use midi::constants::{BoardIndex, LumatoneKeyIndex};
use midi::driver::MidiDriver;
use midi::detect::detect_device;

use env_logger;
use log::debug;
use tokio;

use crate::midi::constants::RGBColor;

#[tokio::main]
async fn main() {
  let default_log_level = "debug";
  let env = env_logger::Env::default().filter_or("RUST_LOG", default_log_level);

  env_logger::init_from_env(env);

  // TODO: fix lifetime issues... maybe tokio scopes?
  let device = detect_device().await.expect("device detection failed");
  let driver = MidiDriver::new(&device).expect("driver creation failed");

  let (done_tx, done_rx) = tokio::sync::oneshot::channel();
  let (command_tx, command_rx) = tokio::sync::mpsc::channel(128);
  let f = driver.run(command_rx, done_rx);

  debug!("starting driver loop");
  let h = tokio::spawn(f);
  debug!("driver loop spawned");

  let commands = vec![
    SetKeyColor::new(BoardIndex::Octave1, LumatoneKeyIndex::unchecked(0), RGBColor::red()),
    SetKeyColor::new(BoardIndex::Octave1, LumatoneKeyIndex::unchecked(1), RGBColor::green()),
    SetKeyColor::new(BoardIndex::Octave1, LumatoneKeyIndex::unchecked(2), RGBColor::blue()),
  ];

  debug!("sending commands");
  for c in commands {
    debug!("sending command");
    command_tx.send(Box::new(c)).await.expect("send error");
  }

  tokio::time::sleep(Duration::from_secs(30)).await;

  debug!("sending done signal");
  done_tx.send(()).expect("error sending done signal");
  tokio::join!(h).0.expect("error joining driver future");
}
