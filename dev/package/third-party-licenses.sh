#!/bin/sh
# Regenerates dev/package/THIRD-PARTY-LICENSES.txt, the notices for the
# third-party code in the packaged program: every Rust crate it links
# (cargo-about, configured by about.toml, from the dev/package/about.hbs
# template), then the vendored C/C++ (dev/package/vendored-licenses). Run it
# after changing dependencies and commit the result; the package scripts
# copy the committed file, so building a package doesn't need cargo-about.
#
# Needs cargo-about: cargo install --locked --features cli cargo-about
set -e
cd "$(dirname "$0")/../.."
out=dev/package/THIRD-PARTY-LICENSES.txt
cargo about generate dev/package/about.hbs -o "$out"
{
    echo
    echo "Vendored libraries (not crates)"
    echo "==============================="
    for f in dev/package/vendored-licenses/*.txt; do
        echo
        echo "------------------------------------------------------------------------------"
        cat "$f"
    done
} >> "$out"
# One line ending throughout (some licence texts come with CRLF).
perl -pi -e 's/\r$//' "$out"
echo "wrote $out"
