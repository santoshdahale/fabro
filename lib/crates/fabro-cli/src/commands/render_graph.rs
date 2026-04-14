use std::io::{Read, Write};

const RENDER_ERROR_PREFIX: &str = "RENDER_ERROR:";

pub(crate) fn execute() -> i32 {
    let mut dot_source = String::new();
    if std::io::stdin().read_to_string(&mut dot_source).is_err() {
        return 1;
    }

    match fabro_graphviz_sys::render_dot_to_svg(&dot_source) {
        Ok(svg) => {
            if std::io::stdout().write_all(&svg).is_err() {
                return 1;
            }
            0
        }
        Err(err) => {
            if write!(std::io::stdout(), "{RENDER_ERROR_PREFIX}{err}").is_err() {
                return 1;
            }
            0
        }
    }
}
