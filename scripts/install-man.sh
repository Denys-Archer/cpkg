#!/usr/bin/env sh
set -eu

PREFIX="${1:-/usr/local}"
MANDIR="${PREFIX}/share/man"

if [ ! -d "docs" ]; then
    echo "Error: 'docs' directory not found. Run this script from the repository root." >&2
    exit 1
fi

install_page() {
    src_file="$1"
    dst_file="$2"

    if [ ! -f "$src_file" ]; then
        return 0
    fi

    if command -v groff >/dev/null 2>&1; then
        groff -man -Tutf8 "$src_file" >/dev/null
    fi

    install -Dm644 "$src_file" "$dst_file"
    echo "Installed $src_file -> $dst_file"
}

install_page "docs/catpm.1" "${MANDIR}/man1/catpm.1"
install_page "docs/cpkg.1" "${MANDIR}/man1/cpkg.1"
install_page "docs/catpm-config.5" "${MANDIR}/man5/catpm-config.5"
install_page "docs/catpm-repository.5" "${MANDIR}/man5/catpm-repository.5"

if command -v mandb >/dev/null 2>&1; then
    mandb -q || true
fi

echo "Done. Man pages installed under ${MANDIR}"
