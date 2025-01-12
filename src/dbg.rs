#[macro_export]
macro_rules! dbg {
    () => {
        #[cfg(feature = "debug")]
        libc_print::libc_dbg!();
    };
    ($val:expr $(,)?) => {
        #[cfg(feature = "debug")]
        libc_print::libc_dbg!($val);
    };
    ($($val:expr),+ $(,)?) => {
        #[cfg(feature = "debug")]
        libc_print::libc_dbg!($($val),+);
    };
}

#[macro_export]
macro_rules! eprintln {
    () => {
        #[cfg(feature = "debug")]
        libc_print::libc_eprintln!();
    };
    ($($arg:tt)*) => {
        #[cfg(feature = "debug")]
        libc_print::libc_eprintln!($($arg)*);
    };
}

#[macro_export]
macro_rules! println {
    () => {
        #[cfg(feature = "debug")]
        libc_print::libc_println!();
    };
    ($($arg:tt)*) => {
        #[cfg(feature = "debug")]
        libc_print::libc_println!($($arg)*);
    };
}
