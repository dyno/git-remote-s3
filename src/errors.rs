error_chain! {
    foreign_links {
        Io(::std::io::Error);
        Fmt(::std::fmt::Error);
    }
}

pub fn is_broken_pipe(error: &Error) -> bool {
    if let Some(io_error) = error.downcast_ref::<std::io::Error>() {
        return io_error.kind() == std::io::ErrorKind::BrokenPipe;
    }
    false
}
