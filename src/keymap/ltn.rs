#![allow(unused)]
use crate::midi::constants::{
  BoardIndex, LumatoneKeyFunction, LumatoneKeyIndex, LumatoneKeyLocation, RGBColor,
};
/// Utilities for working with the .ltn Lumatone preset file format.
///
use std::collections::HashMap;

use ini::Ini;
use num_traits::FromPrimitive;

use super::{tables::{ConfigurationTables, velocity_intervals_to_string}, error::LumatoneKeymapError};

pub struct KeyDefinition {
  pub function: LumatoneKeyFunction,
  pub color: RGBColor,
}

pub struct GeneralOptions {
  pub after_touch_active: bool,
  pub light_on_key_strokes: bool,
  pub invert_foot_controller: bool,
  pub invert_sustain: bool,
  pub expression_controller_sensitivity: u8,
  
  pub config_tables: ConfigurationTables,
}

impl Default for GeneralOptions {
  fn default() -> Self {
    GeneralOptions {
      after_touch_active: false,
      light_on_key_strokes: false,
      invert_foot_controller: false,
      invert_sustain: false,
      expression_controller_sensitivity: 0,
      config_tables: ConfigurationTables::default(),
    }
  }
}

pub struct LumatoneKeyMap {
  keys: HashMap<LumatoneKeyLocation, KeyDefinition>,
  general: GeneralOptions,
}

impl LumatoneKeyMap {
  pub fn new() -> Self {
    LumatoneKeyMap {
      keys: HashMap::new(),
      general: GeneralOptions::default(),
    }
  }

  pub fn set_key<'a>(
    &'a mut self,
    location: LumatoneKeyLocation,
    def: KeyDefinition,
  ) -> &'a mut LumatoneKeyMap {
    self.keys.insert(location, def);
    self
  }

  // TODO: add batch key update fn that takes HashMap or seq of (location, definition) tuples

  pub fn set_global_options<'a>(&'a mut self, opts: GeneralOptions) -> &'a mut LumatoneKeyMap {
    self.general = opts;
    self
  }

  pub fn as_ini(&self) -> Ini {
    let mut conf = Ini::new();

    let bool_str = |b: bool| if b { 1 } else { 0 }.to_string();
    // set general options
    conf
      .with_general_section()
      .set(
        "AfterTouchActive",
        bool_str(self.general.after_touch_active),
      )
      .set(
        "LightOnKeyStrokes",
        bool_str(self.general.light_on_key_strokes),
      )
      .set(
        "InvertFootController",
        bool_str(self.general.invert_foot_controller),
      )
      .set("InvertSustain", bool_str(self.general.invert_sustain))
      .set(
        "ExprCtrlSensivity",
        self.general.expression_controller_sensitivity.to_string(),
      )
      .set("VelocityIntrvlTbl", velocity_intervals_to_string(&self.general.config_tables.velocity_intervals))
      .set("NoteOnOffVelocityCrvTbl", self.general.config_tables.on_off_velocity.to_string())
      .set("FaderConfig", self.general.config_tables.fader_velocity.to_string())
      .set("afterTouchConfig", self.general.config_tables.aftertouch_velocity.to_string())
      .set("LumaTouchConfig", self.general.config_tables.lumatouch_velocity.to_string());

    // Key definitions are split into sections, one for each board / octave
    for b in 1..=5 {
      let board_index: BoardIndex = FromPrimitive::from_u8(b).unwrap();
      let keys = self
        .keys
        .iter()
        .filter(|(loc, _)| loc.board_index() == board_index);

      let section_name = format!("Board{b}");
      for (loc, def) in keys {
        let key_index: u8 = loc.key_index().into();
        let key_type = def.function.key_type_code();

        conf
          .with_section(Some(section_name.clone()))
          .set(
            format!("Key_{key_index}"),
            def.function.note_or_cc_num().to_string(),
          )
          .set(
            format!("Chan_{key_index}"),
            def.function.midi_channel_byte().to_string(),
          )
          .set(format!("Col_{key_index}"), def.color.to_hex_string());

        if key_type != 1 {
          conf
            .with_section(Some(section_name.clone()))
            .set(format!("KTyp_{key_index}"), key_type.to_string());
        }
      }

      // explicitly set any missing keys to "disabled"
      for k in LumatoneKeyIndex::MIN_VALUE..=LumatoneKeyIndex::MAX_VALUE {
        let key_index = LumatoneKeyIndex::unchecked(k);
        let loc = LumatoneKeyLocation(board_index, key_index);
        if self.keys.contains_key(&loc) {
          continue;
        }
        conf
          .with_section(Some(section_name.clone()))
          .set(format!("Key_{key_index}"), "0")
          .set(format!("Chan_{key_index}"), "1")
          .set(format!("Col_{key_index}"), "000000")
          .set(format!("KTyp_{key_index}"), "4");
      }
    }

    conf
  }

  pub fn from_ini_str(source: &str) -> Result<LumatoneKeyMap, LumatoneKeymapError> {
    let ini = Ini::load_from_str(source)?;

    let mut general = GeneralOptions::default();

    todo!()
  }
}

#[cfg(test)]
mod tests {
  use crate::midi::constants::{key_loc_unchecked, LumatoneKeyFunction, MidiChannel, RGBColor};

  use super::{GeneralOptions, KeyDefinition, LumatoneKeyMap};

  #[test]
  fn test_keymap_to_ini() {
    let mut keymap = LumatoneKeyMap::new();

    keymap
      .set_key(
        key_loc_unchecked(1, 0),
        KeyDefinition {
          function: LumatoneKeyFunction::NoteOnOff {
            channel: MidiChannel::default(),
            note_num: 60,
          },
          color: RGBColor(0xff, 0, 0),
        },
      )
      .set_key(
        key_loc_unchecked(2, 0),
        KeyDefinition {
          function: LumatoneKeyFunction::LumaTouch {
            channel: MidiChannel::unchecked(2),
            note_num: 70,
            fader_up_is_null: false,
          },
          color: RGBColor::green(),
        },
      );

    let ini = keymap.as_ini();
    let board_1 = ini.section(Some("Board1".to_string())).unwrap();
    assert_eq!(board_1.get("Key_0"), Some("60"));
    assert_eq!(board_1.get("Chan_0"), Some("1"));
    assert_eq!(board_1.get("Col_0"), Some("ff0000"));
    assert_eq!(board_1.get("KTyp_0"), None); // KTyp is only set if keytype is not NoteOnOff

    let board_2 = ini.section(Some("Board2".to_string())).unwrap();
    assert_eq!(board_2.get("Key_0"), Some("70"));
    assert_eq!(board_2.get("Chan_0"), Some("2"));
    assert_eq!(board_2.get("Col_0"), Some("00ff00"));
    assert_eq!(board_2.get("KTyp_0"), Some("3"));

    // missing keys should have KTyp == 4 (disabled), Key = 0, Chan = 1, Col = 000000
    let board_3 = ini.section(Some("Board3".to_string())).unwrap();
    assert_eq!(board_3.get("Key_10"), Some("0"));
    assert_eq!(board_3.get("Chan_10"), Some("1"));
    assert_eq!(board_3.get("Col_10"), Some("000000"));
    assert_eq!(board_3.get("KTyp_10"), Some("4"));

    let general = ini.general_section();
    assert_eq!(general.get("AfterTouchActive"), Some("0"));
    assert_eq!(general.get("LightOnKeyStrokes"), Some("0"));
    assert_eq!(general.get("InvertFootController"), Some("0"));
    assert_eq!(general.get("InvertSustain"), Some("0"));
    assert_eq!(general.get("ExprCtrlSensivity"), Some("0"));
  }

  #[test]
  fn test_general_opts_to_ini() {
    let mut keymap = LumatoneKeyMap::new();

    keymap.set_global_options(GeneralOptions {
      after_touch_active: true,
      light_on_key_strokes: true,
      invert_foot_controller: true,
      invert_sustain: true,
      expression_controller_sensitivity: 100,
    });

    let ini = keymap.as_ini();
    let general = ini.general_section();
    assert_eq!(general.get("AfterTouchActive"), Some("1"));
    assert_eq!(general.get("LightOnKeyStrokes"), Some("1"));
    assert_eq!(general.get("InvertFootController"), Some("1"));
    assert_eq!(general.get("InvertSustain"), Some("1"));
    assert_eq!(general.get("ExprCtrlSensivity"), Some("100"));
  }
}
