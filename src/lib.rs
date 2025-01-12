#![no_std]
#![feature(slice_split_once)]

mod dbg;
mod detour;
mod util;

use core::{
    alloc::Layout,
    cmp::Ordering,
    mem::{self, MaybeUninit},
    ptr::{self, addr_of},
    slice,
    str::{self, FromStr},
};
use elf::{
    abi,
    dynamic::DynamicTable,
    endian::AnyEndian,
    file::{self, Class, FileHeader},
    hash::{GnuHashTable, SysVHashTable},
    parse::ParseAt,
    segment::{ProgramHeader, SegmentTable},
    string_table::StringTable,
    symbol::SymbolTable,
};
use libc::{dev_t, ino_t};
use spin::Once;

#[doc(hidden)]
pub static mut INITIALIZED: Once = Once::new();

static mut LIBRARIES: Option<&[Library]> = None;
static mut SELF_FILE_ID: Option<FileId> = None;

struct Library<'a> {
    hash: Option<SysVHashTable<'a, AnyEndian>>,
    gnu_hash: Option<GnuHashTable<'a, AnyEndian>>,
    symtab: SymbolTable<'a, AnyEndian>,
    strtab: StringTable<'a>,
    maps: &'a [Mapping],
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct FileId {
    major: dev_t,
    minor: dev_t,
    inode: ino_t,
}

#[derive(Copy, Clone)]
struct Mapping {
    base: usize,
    offset: usize,
    len: usize,
}

#[doc(hidden)]
pub unsafe fn init() {
    let Some(maps) = init_maps() else {
        return;
    };

    let mut buf_ptr = ptr::null_mut();

    #[allow(invalid_value, clippy::uninit_assumed_init)]
    let mut buf_len = MaybeUninit::uninit().assume_init();

    if !init_inner(&mut buf_ptr, &mut buf_len, maps) && !buf_ptr.is_null() {
        util::free(buf_ptr, buf_len);
    }

    util::free(maps.as_ptr().cast_mut().cast(), maps.len());
}

unsafe fn init_maps() -> Option<&'static [u8]> {
    let (buf_ptr, len, mut buf_len) =
        util::read_file(c"/proc/self/maps".as_ptr().cast())?;

    let mut buf_ptr = buf_ptr.cast();
    let _ = util::realloc(&mut buf_ptr, &mut buf_len, len);

    Some(slice::from_raw_parts(buf_ptr.cast(), len))
}

unsafe fn init_inner(
    buf_ptr: &mut *mut (),
    buf_len: &mut usize,
    maps: &[u8],
) -> bool {
    struct DataPointers {
        library_store: *mut Library<'static>,
        maps_store: *mut Mapping,
        tmp_maps_store: *mut (FileId, Mapping),
    }

    let map_count = maps.iter().filter(|x| **x == b'\n').count();

    let layout = Layout::new::<()>();

    let (layout, library_store_off) = layout
        .extend(Layout::array::<Library>(map_count).unwrap_unchecked())
        .unwrap_unchecked();

    let (layout, maps_store_off) = layout
        .extend(Layout::array::<Mapping>(map_count).unwrap_unchecked())
        .unwrap_unchecked();

    let (layout, tmp_maps_store_off) = layout
        .extend(
            Layout::array::<(FileId, Mapping)>(map_count).unwrap_unchecked(),
        )
        .unwrap_unchecked();

    let new_buf_ptr = util::malloc(layout.size());

    if new_buf_ptr.is_null() {
        return false;
    }

    *buf_ptr = new_buf_ptr;
    *buf_len = layout.size();

    let pointers = DataPointers {
        library_store: (*buf_ptr).byte_add(library_store_off).cast(),
        maps_store: (*buf_ptr).byte_add(maps_store_off).cast(),
        tmp_maps_store: (*buf_ptr).byte_add(tmp_maps_store_off).cast(),
    };

    let mut saved_map_count = 0;

    for line in maps.split(|x| *x == b'\n').filter(|x| !x.is_empty()) {
        let Some((file_id, map)) = parse_mapping(line) else {
            continue;
        };

        if SELF_FILE_ID == Some(file_id) {
            continue;
        }

        if (map.base..map.base.unchecked_add(map.len))
            .contains(&(addr_of!(INITIALIZED) as _))
        {
            SELF_FILE_ID = Some(file_id);

            continue;
        }

        *pointers.tmp_maps_store.add(saved_map_count) = (file_id, map);
        saved_map_count = saved_map_count.unchecked_add(1);
    }

    let tmp_maps: &mut [(FileId, Mapping)] = slice::from_raw_parts_mut(
        pointers.tmp_maps_store.cast(),
        saved_map_count,
    );

    tmp_maps.sort_unstable_by(|a, b| {
        (a.0 == b.0)
            .then(|| a.1.base.cmp(&b.1.base))
            .unwrap_or(Ordering::Greater)
    });

    let mut base_map_idx = None;
    let mut library_count = 0;
    let mut iter = tmp_maps.iter().enumerate().peekable();

    while let Some((i, (file_id, map))) = iter.next() {
        if SELF_FILE_ID == Some(*file_id) {
            continue;
        }

        *pointers.maps_store.add(i) = *map;

        if map.offset == 0 {
            base_map_idx = Some(i);
        }

        let Some(base_map_idx) = base_map_idx else {
            continue;
        };

        if iter.peek().is_some_and(|(_, (_, map))| map.offset == 0) {
            if let Some(lib) = get_library(slice::from_raw_parts(
                pointers.maps_store.add(base_map_idx),
                i.unchecked_sub(base_map_idx).unchecked_add(1),
            )) {
                *pointers.library_store.add(library_count) = lib;
                library_count = library_count.unchecked_add(1);
            }
        }
    }

    LIBRARIES =
        Some(slice::from_raw_parts(pointers.library_store, library_count));

    true
}

unsafe fn parse_mapping(line: &[u8]) -> Option<(FileId, Mapping)> {
    let mut iter = line.split(|x| *x == b' ');

    let (start, end) = iter
        .next()
        .unwrap_unchecked()
        .split_once(|x| *x == b'-')
        .unwrap_unchecked();

    let start = usize::from_str_radix(str::from_utf8_unchecked(start), 16)
        .unwrap_unchecked();

    let end = usize::from_str_radix(str::from_utf8_unchecked(end), 16)
        .unwrap_unchecked();

    // TODO: Check if offset can be larger than `usize`
    let offset = usize::from_str_radix(
        str::from_utf8_unchecked(iter.nth(1).unwrap_unchecked()),
        16,
    )
    .unwrap_unchecked();

    let (major, minor) = iter
        .next()
        .unwrap_unchecked()
        .split_once(|x| *x == b':')
        .unwrap_unchecked();

    let major = dev_t::from_str_radix(str::from_utf8_unchecked(major), 16)
        .unwrap_unchecked();

    let minor = dev_t::from_str_radix(str::from_utf8_unchecked(minor), 16)
        .unwrap_unchecked();

    if major == 0 && minor == 0 {
        return None;
    }

    let inode = ino_t::from_str(str::from_utf8_unchecked(
        iter.next().unwrap_unchecked(),
    ))
    .unwrap_unchecked();

    if inode == 0 {
        return None;
    }

    Some((
        FileId {
            major,
            minor,
            inode,
        },
        Mapping {
            base: start,
            offset,
            len: end.unchecked_sub(start),
        },
    ))
}

fn get_library(maps: &[Mapping]) -> Option<Library> {
    let map = unsafe { maps.get_unchecked(0) };
    let data = unsafe { slice::from_raw_parts(map.base as _, map.len) };

    let ident =
        file::parse_ident::<AnyEndian>(data.get(..abi::EI_NIDENT)?).ok()?;

    let tail_addr = abi::EI_NIDENT;

    let tail_end = match ident.1 {
        Class::ELF32 => unsafe {
            tail_addr.unchecked_add(file::ELF32_EHDR_TAILSIZE)
        },
        Class::ELF64 => unsafe {
            tail_addr.unchecked_add(file::ELF64_EHDR_TAILSIZE)
        },
    };

    let ehdr =
        FileHeader::parse_tail(ident, data.get(tail_addr..tail_end)?).ok()?;

    if ehdr.e_type != abi::ET_DYN {
        return None;
    }

    let entsize =
        ProgramHeader::validate_entsize(ehdr.class, ehdr.e_phentsize as _)
            .ok()?;

    let phoff = ehdr.e_phoff as usize;
    let size = entsize.checked_mul(ehdr.e_phnum as usize)?;
    let end = phoff.checked_add(size)?;
    let buf = data.get(phoff..end)?;
    let segments = SegmentTable::new(ehdr.endianness, ehdr.class, buf);
    let dyn_hdr = segments.iter().find(|x| x.p_type == abi::PT_DYNAMIC)?;
    let dyn_addr = (map.base as u64).checked_add(dyn_hdr.p_vaddr)?;

    let dyn_tab = DynamicTable::new(ehdr.endianness, ehdr.class, unsafe {
        slice::from_raw_parts(dyn_addr as _, dyn_hdr.p_filesz as _)
    });

    let (mut symtab_addr, mut strtab_addr, mut hash_addr, mut gnu_hash_addr) =
        Default::default();

    for ent in dyn_tab.iter() {
        let mut pairs = [
            (abi::DT_SYMTAB, &mut symtab_addr),
            (abi::DT_STRTAB, &mut strtab_addr),
            (abi::DT_HASH, &mut hash_addr),
            (abi::DT_GNU_HASH, &mut gnu_hash_addr),
        ];

        for (kind, val) in pairs.iter_mut() {
            if ent.d_tag != *kind {
                continue;
            }

            let mut ptr = ent.d_ptr() as usize;

            // TODO: Find a proper way to check for relative/absolute address
            if let Some(adj_ptr) = ptr.checked_sub(map.base) {
                ptr = adj_ptr;
            }

            **val = Some(ptr);

            break;
        }

        if pairs.iter().all(|x| x.1.is_some()) {
            break;
        }
    }

    let (symtab_addr, strtab_addr) = (symtab_addr?, strtab_addr?);

    Some(Library {
        hash: hash_addr.and_then(|x| {
            SysVHashTable::new(ehdr.endianness, ehdr.class, unsafe {
                data.get_unchecked(x..)
            })
            .ok()
        }),
        gnu_hash: gnu_hash_addr.and_then(|x| {
            GnuHashTable::new(ehdr.endianness, ehdr.class, unsafe {
                data.get_unchecked(x..)
            })
            .ok()
        }),
        strtab: StringTable::new(unsafe { data.get_unchecked(strtab_addr..) }),
        symtab: SymbolTable::new(ehdr.endianness, ehdr.class, unsafe {
            data.get_unchecked(symtab_addr..)
        }),
        maps,
    })
}

#[doc(hidden)]
pub unsafe fn lookup_symbol<F: Copy>(
    name: &[u8],
    save: &mut Option<F>,
) -> Option<F> {
    if let Some(func) = save {
        return Some(*func);
    }

    LIBRARIES?
        .iter()
        .find_map(|lib| lookup_lib_symbol(name, lib))
        .inspect(|x| *save = Some(*x))
}

unsafe fn lookup_lib_symbol<F>(name: &[u8], lib: &Library) -> Option<F> {
    let (_, sym) = lib
        .hash
        .as_ref()
        .and_then(|x| x.find(name, &lib.symtab, &lib.strtab).ok().flatten())
        .or_else(|| {
            lib.gnu_hash.as_ref().and_then(|x| {
                x.find(name, &lib.symtab, &lib.strtab).ok().flatten()
            })
        })?;

    if sym.st_value == 0 || sym.st_shndx == abi::SHN_UNDEF {
        return None;
    }

    let mut addr = None;

    for map in lib.maps {
        if (map.offset..map.offset.unchecked_add(map.len))
            .contains(&(sym.st_value as usize))
        {
            addr = Some(map.base.unchecked_add(
                (sym.st_value as usize).unchecked_sub(map.offset),
            ));

            break;
        }
    }

    let addr = addr?;

    Some(mem::transmute_copy(&(addr as *const ())))
}
