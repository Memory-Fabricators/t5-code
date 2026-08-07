//! Port of `src/utils/process.ts` (stub).

pub fn exit_with_error(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}
