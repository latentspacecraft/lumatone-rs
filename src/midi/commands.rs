#![allow(dead_code)]

use crate::midi::sysex::message_command_id;

use super::{
  constants::{BoardIndex, CommandId as CMD, TEST_ECHO},
  error::LumatoneMidiError,
  sysex::{
    create_extended_key_color_sysex, create_sysex, is_lumatone_message, message_payload,
    EncodedSysex,
  },
};

/// CMD 0x0: Send a single key's functional configuration
pub fn set_key_function_parameters(
  board_index: BoardIndex,
  key_index: u8,
  note_or_cc_num: u8,
  midi_channel: u8,
  key_type: u8,
  fader_up_is_null: bool,
) -> EncodedSysex {
  let channel = (midi_channel - 1) & 0xf;
  let type_byte: u8 = if fader_up_is_null {
    (1 << 4) | key_type
  } else {
    key_type
  };
  create_sysex(
    board_index,
    CMD::ChangeKeyNote,
    vec![key_index, note_or_cc_num, channel, type_byte],
  )
}

/// CMD 0x01: Send a single key's LED channel intensities
pub fn set_key_light_parameters(
  board_index: BoardIndex,
  key_index: u8,
  red: u8,
  green: u8,
  blue: u8,
) -> EncodedSysex {
  create_extended_key_color_sysex(board_index, CMD::SetKeyColour, key_index, red, green, blue)
}

/// CMD 0x02: Save current configuration to a specified preset button index
pub fn save_program(preset_number: u8) -> Result<EncodedSysex, LumatoneMidiError> {
  if preset_number > 9 {
    return Err(LumatoneMidiError::InvalidCommandInput(
      CMD::SaveProgram,
      "invalid input: max preset number is 9".to_string(),
    ));
  }

  Ok(create_sysex(
    BoardIndex::Server,
    CMD::SaveProgram,
    vec![preset_number],
  ))
}

/// Constructs a "ping" message, with the given value encoded into the payload.
/// Note that value is truncated to 28 bits.
pub fn ping(value: u32) -> EncodedSysex {
  let val = value & 0xfffffff; // limit to 28 bits
  create_sysex(
    BoardIndex::Server,
    CMD::LumaPing,
    vec![
      TEST_ECHO,
      ((val >> 14) & 0x7f) as u8,
      ((val >> 7) & 0x7f) as u8,
      (val & 0x7f) as u8,
    ],
  )
}

/// Attempts to decode a sysex message as a "ping" response,
/// returning the encoded payload value on success.
pub fn decode_ping(msg: &[u8]) -> Result<u32, LumatoneMidiError> {
  if !is_lumatone_message(msg) {
    return Err(LumatoneMidiError::NotLumatoneMessage(msg.to_vec()));
  }

  let cmd_id = message_command_id(msg)?;
  if cmd_id != CMD::LumaPing {
    return Err(LumatoneMidiError::UnexpectedCommandId {
      expected: CMD::LumaPing,
      actual: cmd_id,
    });
  }

  let payload = message_payload(msg)?;
  if payload.len() < 4 {
    return Err(LumatoneMidiError::MessagePayloadTooShort {
      expected: 4,
      actual: payload.len(),
    });
  }

  if payload[0] != TEST_ECHO {
    return Err(LumatoneMidiError::InvalidResponseMessage(
      "ping response has invalid echo flag value".to_string(),
    ));
  }

  let value: u32 = ((payload[1] as u32) << 14) | ((payload[2] as u32) << 7) | (payload[3] as u32);
  Ok(value)
}

// TODO: add remaining commands
