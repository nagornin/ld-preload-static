#[macro_export]
macro_rules! detour {
    (
        $name:ident($($arg_name:ident: $arg_type:ty),*) -> $ret:ty,
        $body:block,
        $fallback:block,
    ) => {
        #[allow(non_snake_case)]
        #[no_mangle]
        unsafe extern "C" fn $name($($arg_name: $arg_type),*) -> $ret {
            $crate::INITIALIZED.call_once(|| $crate::init());

            static mut REAL_FUNC: Option<
                unsafe extern "C" fn($($arg_name: $arg_type),*) -> $ret
            > = None;

            if let Some($name) = $crate::lookup_symbol(
                stringify!($name).as_bytes(),
                &mut REAL_FUNC
            ) {
                return $body;
            }

            $fallback
        }
    }
}

#[macro_export]
macro_rules! detour_multi {
    (
        $orig:ident,
        $body:block,
        $fallback:block,
        $($name:ident($($arg_name:ident: $arg_type:ty),*) -> $ret:ty),*
    ) => {
        $(
            detour!(
                $name,
                {
                    let $orig = || $name($($arg_name),*);

                    $body
                },
                $fallback,
                $ret,
                $($arg_name: $arg_type),*
            );
        )*
    };
}
