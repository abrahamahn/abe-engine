#!/bin/sh
# Build the Metal player and renderer for macOS, and optionally check them on this Mac.
#
#   scripts/apple/build-mac.sh            build ganbate-player and ganbate-render (release)
#   scripts/apple/build-mac.sh --shaders  also compile every renderer shader's MSL with
#                                         Apple's offline compiler (needs Xcode's Metal
#                                         toolchain; skipped with a note otherwise)
#   scripts/apple/build-mac.sh --test     also run the backend and renderer driver tests on
#                                         Metal (tests/backend.rs, tests/render.rs)
#
# Needs the Xcode command-line tools (xcode-select --install) and Rust (rustup). Shaders are
# compiled from MSL at run time by the Metal framework, so running needs no Xcode.
set -eu
cd "$(dirname "$0")/../.."

shaders=0
tests=0
for arg in "$@"; do
    case "$arg" in
    --shaders) shaders=1 ;;
    --test) tests=1 ;;
    -h | --help)
        sed -n '2,13p' "$0"
        exit 0
        ;;
    *)
        echo "unknown option $arg" >&2
        exit 2
        ;;
    esac
done

if [ "$(uname -s)" != Darwin ]; then
    echo "build-mac.sh runs on macOS" >&2
    exit 1
fi
if ! xcrun --find clang >/dev/null 2>&1; then
    echo "The Xcode command-line tools are missing: run xcode-select --install" >&2
    exit 1
fi

target=aarch64-apple-darwin
[ "$(uname -m)" = x86_64 ] && target=x86_64-apple-darwin
if command -v rustup >/dev/null 2>&1; then
    rustup target add "$target" >/dev/null
fi

cargo build --release --locked --target "$target" --bin ganbate-player --bin ganbate-render
out="${CARGO_TARGET_DIR:-target}/$target/release"
echo "Built $out/ganbate-player and $out/ganbate-render"

if [ "$shaders" = 1 ]; then
    dir="${CARGO_TARGET_DIR:-target}/msl"
    mkdir -p "$dir"
    cargo build --release --locked -p ganbate-shader --example ganbate-shaderc
    shaderc="${CARGO_TARGET_DIR:-target}/release/examples/ganbate-shaderc"
    metal=""
    if xcrun -sdk macosx --find metal >/dev/null 2>&1; then
        metal="xcrun -sdk macosx metal"
    else
        echo "note: Apple's offline Metal compiler is not installed (Xcode > Settings >"
        echo "      Components > Metal Toolchain); writing the MSL without compiling it."
    fi
    failed=0
    for name in scene world_fx post; do
        src="$dir/$name.wgsl"
        cat src/render/shaders/common.wgsl "src/render/shaders/$name.wgsl" >"$src"
        grep -oE '@(vertex|fragment|compute)[^f]*fn [A-Za-z0-9_]+' "$src" |
            sed -E 's/@([a-z]+).*fn ([A-Za-z0-9_]+)/\1 \2/' >"$dir/$name.entries"
        while read -r stage entry; do
            file="$dir/$name.$entry.metal"
            "$shaderc" "$src" "$stage" "$entry" "$file" >/dev/null
            if [ -n "$metal" ]; then
                if $metal -c "$file" -o "$dir/$name.$entry.air" 2>"$dir/$name.$entry.log"; then
                    echo "ok    $name.wgsl $entry"
                else
                    failed=1
                    echo "FAIL  $name.wgsl $entry (see $dir/$name.$entry.log)"
                    sed -n '1,20p' "$dir/$name.$entry.log"
                fi
            else
                echo "wrote $file"
            fi
        done <"$dir/$name.entries"
    done
    [ "$failed" = 0 ] || exit 1
fi

if [ "$tests" = 1 ]; then
    # With Metal's API validation. First the backend tests (small WGSL programs whose results
    # are known; the same tests pass on Vulkan), then offscreen renders of a synthetic scene.
    export MTL_DEBUG_LAYER="${MTL_DEBUG_LAYER:-1}"
    cargo test --release --locked --target "$target" --test backend -- --ignored --test-threads=1
    cargo test --release --locked --target "$target" --test render -- --ignored --test-threads=1
fi
