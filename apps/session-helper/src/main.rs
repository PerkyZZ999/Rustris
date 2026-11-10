use std::process::ExitCode;
fn main() -> ExitCode {
    match rustris_core::sessions::native::run_helper(std::env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Rustris session: {error}");
            ExitCode::FAILURE
        }
    }
}
