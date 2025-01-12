use cfg_if::cfg_if;
use core::ptr;
use libc::{
    MAP_ANONYMOUS, MAP_FAILED, MAP_PRIVATE, O_RDONLY, PROT_READ, PROT_WRITE,
};
use syscalls::{raw_syscall, Sysno};

#[cfg(target_arch = "aarch64")]
use libc::AT_FDCWD;

const READ_FILE_INITIAL_BUF_SIZE: usize = 8192;

#[must_use]
pub unsafe fn open(path: *const u8, flags: i32, mode: u32) -> i32 {
    cfg_if! {
        if #[cfg(target_arch = "aarch64")] {
            raw_syscall!(Sysno::openat, AT_FDCWD, path, flags, mode) as i32
        } else {
            raw_syscall!(Sysno::open, path, flags, mode) as i32
        }
    }
}

#[must_use]
pub unsafe fn read(fd: i32, buf: *mut (), len: usize) -> isize {
    raw_syscall!(Sysno::read, fd, buf, len) as _
}

#[must_use]
pub fn close(fd: i32) -> i32 {
    unsafe { raw_syscall!(Sysno::close, fd) as i32 }
}

#[must_use]
pub unsafe fn malloc(length: usize) -> *mut () {
    let ptr = mmap(
        0,
        length,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANONYMOUS,
        -1,
        0,
    );

    if ptr == MAP_FAILED.cast() {
        ptr::null_mut()
    } else {
        ptr
    }
}

#[must_use]
unsafe fn mmap(
    addr: usize,
    length: usize,
    prot: i32,
    flags: i32,
    fd: i32,
    offset: i32,
) -> *mut () {
    cfg_if! {
        if #[cfg(target_arch = "arm")] {
            raw_syscall!(
                Sysno::mmap2,
                addr,
                length,
                prot,
                flags,
                fd,
                offset
            ) as _
        } else if #[cfg(target_arch = "x86")] {
            raw_syscall!(
                Sysno::mmap,
                [
                    addr,
                    length,
                    prot as _,
                    flags as _,
                    fd as _,
                    offset as _
                ].as_ptr()
            ) as _
        } else {
            raw_syscall!(
                Sysno::mmap,
                addr,
                length,
                prot,
                flags,
                fd,
                offset
            ) as _
        }
    }
}

#[must_use]
pub unsafe fn realloc(
    ptr: &mut *mut (),
    len: &mut usize,
    new_len: usize,
) -> bool {
    let new_ptr = mremap(*ptr, *len, new_len, 0);

    if new_ptr == MAP_FAILED.cast() {
        return false;
    }

    *ptr = new_ptr;
    *len = new_len;

    true
}

#[must_use]
unsafe fn mremap(
    old_addr: *mut (),
    old_size: usize,
    new_size: usize,
    flags: i32,
) -> *mut () {
    raw_syscall!(Sysno::mremap, old_addr, old_size, new_size, flags) as _
}

pub unsafe fn free(ptr: *mut (), length: usize) {
    let _ = munmap(ptr, length);
}

#[must_use]
unsafe fn munmap(addr: *mut (), length: usize) -> i32 {
    raw_syscall!(Sysno::munmap, addr, length) as _
}

pub unsafe fn read_file(path: *const u8) -> Option<(*mut u8, usize, usize)> {
    let mut buf_ptr = malloc(READ_FILE_INITIAL_BUF_SIZE);

    if buf_ptr == MAP_FAILED.cast() {
        return None;
    }

    let mut buf_len = READ_FILE_INITIAL_BUF_SIZE;
    let mut fd = open(path, O_RDONLY, 0);

    if fd < 0 {
        return None;
    }

    let res = read_file_inner(&mut buf_ptr, &mut buf_len, &mut fd, path);
    let _ = close(fd);

    if let Some(info) = res {
        Some(info)
    } else {
        let _ = munmap(buf_ptr, buf_len);

        None
    }
}

unsafe fn read_file_inner(
    buf_ptr: &mut *mut (),
    buf_len: &mut usize,
    fd: &mut i32,
    path: *const u8,
) -> Option<(*mut u8, usize, usize)> {
    let mut len = 0;

    loop {
        let read =
            read(*fd, (*buf_ptr).byte_add(len), (*buf_len).unchecked_sub(len));

        if read < 0 {
            return None;
        }

        if read == 0 {
            break;
        }

        len = len.unchecked_add(read as _);

        if len == (*buf_len) {
            if !realloc(buf_ptr, buf_len, buf_len.unchecked_mul(2)) {
                return None;
            }

            let _ = close(*fd);
            let new_fd = open(path, O_RDONLY, 0);

            if new_fd < 0 {
                return None;
            }

            len = 0;
            *fd = new_fd;
        }
    }

    Some(((*buf_ptr).cast(), len, *buf_len))
}
