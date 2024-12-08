#![doc = include_str!("../../README.md")]
#![warn(
    clippy::pedantic,
    clippy::clone_on_ref_ptr,
    clippy::create_dir,
    clippy::filetype_is_file,
    clippy::fn_to_numeric_cast_any,
    clippy::if_then_some_else_none,
    missing_docs,
    clippy::missing_docs_in_private_items,
    missing_copy_implementations,
    missing_debug_implementations,
    clippy::missing_const_for_fn,
    clippy::mixed_read_write_in_expression,
    clippy::partial_pub_fields,
    clippy::same_name_method,
    clippy::str_to_string,
    clippy::suspicious_xor_used_as_pow,
    clippy::try_err,
    clippy::unneeded_field_pattern,
    clippy::use_debug,
    clippy::verbose_file_reads,
    clippy::manual_saturating_arithmetic
)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unreachable,
    clippy::unimplemented,
    clippy::todo,
    clippy::dbg_macro,
    clippy::exit,
    clippy::panic_in_result_fn,
    clippy::tests_outside_test_module,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    unsafe_code
)]
#![feature(iter_intersperse)]
#![feature(trait_upcasting)]

mod app;
pub mod editor;
mod logging;

use arcane_core::Result;
use crossterm::event::{
    KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;

include!(concat!(env!("OUT_DIR"), "/auto_load.rs"));

fn main() -> Result<()> {
    let _logs = logging::setup()?;

    let result = start_application();
    ratatui::restore();

    result
}

/// Create terminal and start the app
fn start_application() -> Result<()> {
    let mut terminal = ratatui::init();
    execute!(
        terminal.backend_mut(),
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        )
    )
    .unwrap();
    app::App::new().run(&mut terminal)?;
    execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags).unwrap();

    Ok(())
}
