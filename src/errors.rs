error_chain! {
    foreign_links {
        Io(::std::io::Error);
        Fmt(::std::fmt::Error);
    }
}

pub fn is_broken_pipe(error: &Error) -> bool {
    let err_string = error.to_string();
    err_string.contains("Broken pipe") || err_string.contains("broken pipe")
}
