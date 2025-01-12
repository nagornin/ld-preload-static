#![no_std]
#![allow(unsafe_op_in_unsafe_fn)]

use core::ptr;
use libc::{c_int, timespec, utsname};

ld_preload_static::detour!(
    nanosleep(req: *const timespec, rem: *mut timespec) -> c_int,
    {
        let req = timespec {
            tv_sec: (*req).tv_sec / 2,
            tv_nsec: 0
        };

        nanosleep(ptr::addr_of!(req), rem)
    },
    { panic!(); },
);

ld_preload_static::detour!(
    uname(buf: *mut utsname) -> c_int,
    {
        let ret = uname(buf);

        if ret != -1 {
            let sysname = c"Windows NT";
            let release = c"10.0.26100.2605";

            for (src, dest) in [
                (sysname, &mut (*buf).sysname),
                (release, &mut (*buf).release),
            ] {
                ptr::copy_nonoverlapping(
                    src.as_ptr(),
                    dest.as_mut_ptr(),
                    src.to_bytes_with_nul().len()
                );
            }
        }

        ret
    },
    { panic!(); },
);

#[panic_handler]
unsafe fn handle_panic(_: &core::panic::PanicInfo) -> ! {
    core::hint::unreachable_unchecked();
}
