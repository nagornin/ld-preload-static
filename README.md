# Static LD_PRELOAD

Detour functions with LD_PRELOAD, but without `libdl`! The resulting library is fully static.

## Overview

Normally if you want to call the original function when using LD_PRELOAD, you have to link `libdl` to use e.g. `dlsym`, which makes your library dynamically linked.

This project allows you to create a fully static shared library that doesn't use `libdl` to resolve functions. Instead, it parses `/proc/self/maps` and resolves functions manually by [parsing the ELF structures](https://docs.rs/elf).

## Features

- Lazy initialization on first detoured function call
- Only uses Linux syscalls
- Should work on multiple architectures

## Usage

An example library is located in the `detour/` folder. You can build the shared library with `./build.sh`

```
$ cd target/release
$ LD_PRELOAD=libdetour.so uname -sro
Windows NT 10.0.26100.2605 GNU/Linux
$ /bin/time -f '%E' env LD_PRELOAD=libdetour.so sleep 8
0:04.00
$ ldd libdetour.so
        statically linked
$ du -h libdetour.so
36K     libdetour.so
```
