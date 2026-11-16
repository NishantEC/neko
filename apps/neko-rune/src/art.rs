//! Home's art: an original night scene drawn by a GPU shader at an effect slot (polish spec §3).
//! Placeholder until the shader lands in the next commit: the slot exists and draws nothing.

use rune::El;

use crate::app::App;
use crate::theme::Neko;

/// The effect slot's mark.
pub const MARK: u32 = 0x4e45_4b01;

pub fn slot(_s: &App, t: &Neko) -> El {
    rune::effect(MARK).effect_data([if t.dark { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0])
}
