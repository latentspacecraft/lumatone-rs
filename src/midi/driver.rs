#![allow(dead_code)]
use std::{error::Error, pin::Pin, time::Duration};
use super::{sysex::EncodedSysex, device::{LumatoneDevice, LumatoneIO}, error::LumatoneMidiError};

use log::{warn, debug, info, error};
use tokio::{sync::{mpsc, oneshot}, time::{sleep, Sleep}};

// state machine design is based around this example: https://play.rust-lang.org/?gist=ee3e4df093c136ced7b394dc7ffb78e1&version=stable&backtrace=0
// linked from "Pretty State Machine Patterns in Rust": https://hoverbear.org/blog/rust-state-machine-pattern/

#[derive(Debug)]
enum State {
  Idle,
  ProcessingQueue { send_queue: Vec<EncodedSysex> },
  AwaitingResponse { send_queue: Vec<EncodedSysex>, command_sent: EncodedSysex },
  DeviceBusy { send_queue: Vec<EncodedSysex>, to_retry: EncodedSysex },
  Failed(LumatoneMidiError),
}

/// Actions are inputs into the state machine. Actions may trigger state transitions.
#[derive(Debug)]
enum Action {
  SubmitCommand(EncodedSysex),
  MessageSent(EncodedSysex),
  MessageReceived(EncodedSysex),
  ResponseTimedOut,
  ReadyToRetry,
}

/// Effects are requests from the state machine to "do something" in the outside world.
#[derive(Debug)]
enum Effect {
  SendMidiMessage(EncodedSysex),
  StartReceiveTimeout,
  StartRetryTimeout,
}


impl State {

  fn next(self, action: Action) -> State {
    use State::*;
    use Action::*;

    match (action, self) {
      (SubmitCommand(msg), Idle) => {
        // Queue up message to send, switch to "processing state"
        ProcessingQueue { send_queue: vec![msg] }
      },

      (SubmitCommand(msg), AwaitingResponse { send_queue , command_sent }) => {
        // add new command to the send_queue
        let mut q = send_queue.clone();
        q.push(msg);
        AwaitingResponse { send_queue: q, command_sent: command_sent }
      },

      (SubmitCommand(msg), DeviceBusy { send_queue, to_retry }) => {
        // add new command to the send queue
        let mut q = send_queue.clone();
        q.push(msg);
        DeviceBusy { send_queue: q, to_retry: to_retry }
      },

      (MessageSent(msg), ProcessingQueue { send_queue }) => {
        let send_queue = send_queue[1..].to_vec();
        AwaitingResponse { send_queue: send_queue, command_sent: msg }
      },

      (MessageReceived(_), AwaitingResponse { send_queue, command_sent: _ }) => {
        // TODO: check if received message is in response to command_sent
        //       if so, notify / log success
        //       if not, notify / log unexpected message
        //       if response says device is busy, enter DeviceBusy state

        if send_queue.is_empty() {
          Idle
        } else {
          ProcessingQueue { send_queue: send_queue }
        }
      },

      (MessageReceived(msg), state) => {
        warn!("Message received when not awaiting response: {:?}", msg);
        state
      },

      (ResponseTimedOut, AwaitingResponse { send_queue, command_sent }) => {
        warn!("Timed out waiting for response to msg: {:?}", command_sent);

        if send_queue.is_empty() {
          Idle
        } else {
          ProcessingQueue { send_queue: send_queue }
        }
      },

      (ResponseTimedOut, state) => {
        warn!("Response timeout action received, but not awaiting response");
        state
      },

      (ReadyToRetry, DeviceBusy { send_queue, to_retry }) => {
        let mut q = vec![to_retry];
        q.extend(send_queue);

        ProcessingQueue { send_queue: q }
      },

      (ReadyToRetry, state) => {
        warn!("ReadyToRetry action received but not in DeviceBusy state");
        state
      }

      (action, state) => {
        let msg = format!("invalid action {:?} for current state {:?}", action, state);
        Failed(LumatoneMidiError::InvalidStateTransition(msg))
      }
    }
  }

  /// Each state can perform an optional Effect when it's entered. Effects may result in new Actions, which can then trigger a new State transition.
  fn enter(&mut self) -> Option<Effect> { 
    use State::*;
    use Effect::*;

    match &*self {
      Idle => { None },
      ProcessingQueue { send_queue } => {
        let msg = send_queue[0].clone();
          Some(SendMidiMessage(msg))
        },
      DeviceBusy { send_queue: _, to_retry: _ } => {
        Some(StartRetryTimeout)
      },
      AwaitingResponse { send_queue: _, command_sent: _ } => {
        Some(StartReceiveTimeout)
      },
      Failed(err) => {
        warn!("midi driver - unrecoverable error: {err}");
        None
      }
    }
  }
}


pub struct MidiDriver {
  device_io: LumatoneIO,
  receive_timeout: Option<Pin<Box<Sleep>>>,
  retry_timeout: Option<Pin<Box<Sleep>>>,
}

impl MidiDriver {

  pub fn new(device: &LumatoneDevice) -> Result<Self, LumatoneMidiError> {
    let device_io = device.connect()?;
    Ok(MidiDriver { 
      device_io,
      receive_timeout: None,
      retry_timeout: None,
    })
  }

  /// Performs some Effect. On success, returns an Option<Action> to potentially trigger a state transition.
  fn perform_effect(&mut self, effect: Effect) -> Result<Option<Action>, Box<dyn Error>> {
    use Effect::*;
    use Action::*;
    let action = match effect {
      SendMidiMessage(msg) => {
        self.device_io.send(&msg)?;
        Some(MessageSent(msg))
      },

      StartReceiveTimeout => {
        let timeout_sec = 30;
        let timeout = sleep(Duration::from_secs(timeout_sec));
        self.receive_timeout = Some(Box::pin(timeout));
        None
      },

      StartRetryTimeout => {
        let timeout_sec = 3;
        let timeout = sleep(Duration::from_secs(timeout_sec));
        self.retry_timeout = Some(Box::pin(timeout));       
        None
      }
    };
    Ok(action)
  }

  pub async fn run(mut self, mut commands: mpsc::Receiver<EncodedSysex>, mut done_signal: oneshot::Receiver<()>) {

    let mut state = State::Idle;
    loop {

      // bail out if instructed
      if done_signal.try_recv().is_ok() {
        debug!("done signal received, exiting");
        break;
      }
      
      // if either timeout is None, use a timeout with Duration::MAX, to make the select! logic a bit simpler
      let mut receive_timeout = &mut Box::pin(sleep(Duration::MAX));
      if let Some(t) = &mut self.receive_timeout {
        receive_timeout = t;
      }

      let mut retry_timeout = &mut Box::pin(sleep(Duration::MAX));
      if let Some(t) = &mut self.retry_timeout {
        retry_timeout = t;
      }

      // There are two incoming streams of information: incoming midi messages, 
      // and incoming commands (requests to send out midi messages)
      // There are also two timeouts: receive_timeout for when we're waiting for a response to a command,
      // and retry_timeout for when we're waiting to re-send a command (because the device was busy last time).
      // 
      // This select pulls whatever is available next and maps it to an Action that will advance the state machine.
      let a = tokio::select! {
        _ = receive_timeout => {
          info!("receive timeout triggered");
          Action::ResponseTimedOut
        },

        _ = retry_timeout => {
          info!("retry timeout triggered");
          Action::ReadyToRetry
        },

        Some(msg) = self.device_io.incoming_messages.recv() => {
          info!("message received, forwarding to state machine");
          Action::MessageReceived(msg)
        }

        Some(cmd) = commands.recv() => {
          Action::SubmitCommand(cmd)
        }
      };

      // Transition to next state based on action
      state = state.next(a);

      if let State::Failed(err) = state { 
        // return Err(err);
        error!("state machine error: {err}");
        break
      }

      // The new state's `enter` fn may return an Effect.
      // If so, run it and apply any Actions returned.
      if let Some(effect) = state.enter() {
        match self.perform_effect(effect) {
          Ok(Some(action)) => { 
            state = state.next(action);
            if let State::Failed(err) = state { 
              error!("state machine error: {err}");
              break;
            }
          },
          Err(err) => {
            // warn!("error performing effect: {}", err);
            error!("state machine error: {err}");
            break;
          }
          _ => {
            // No error, but nothing to do
          }
        }
      }
    }

    // Ok(())
  }

}