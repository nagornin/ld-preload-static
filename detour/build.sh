#!/bin/sh

set -e

cd "$(dirname "$(realpath "$0")")"

exec cargo rustc --release \
    -Z build-std=core \
    -Z build-std-features=compiler-builtins-mem \
    -- -C link-arg=-nostdlib
